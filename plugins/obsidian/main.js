// SPDX-License-Identifier: MIT
// Ibis for Obsidian: one command searches your references with the `ibis`
// command and inserts the citation where you are writing, in the format Ibis
// uses for Obsidian (Pandoc unless you chose another). Plain JavaScript that
// Obsidian loads as it is, with no build step and no dependencies.
"use strict";

const { Notice, Plugin, PluginSettingTab, Setting, SuggestModal } = require("obsidian");
const { execFile } = require("child_process");
const { homedir } = require("os");
const { join } = require("path");

// Obsidian started from the Dock or Start menu may not see the PATH a terminal
// has, so the places Ibis and Homebrew install the command are tried too.
const COMMANDS = [
  "ibis",
  join(homedir(), ".local", "bin", "ibis"),
  "/opt/homebrew/bin/ibis",
  "/usr/local/bin/ibis",
];
// Obsidian's identity in Ibis's app list, where a per-app format is kept.
const APP_ID = process.platform === "win32" ? "obsidian.exe" : "md.obsidian";
const DEFAULTS = { command: "", file: "", zotero: false };
const NO_MATCHES = "No matching references";
const NOT_INSTALLED =
  "Ibis: the ibis command was not found. Install it in Ibis › Settings › AI Apps › Command line, or set its path in this plugin's settings.";

/** Runs `ibis ARGS`; exit 1 means findings and still carries the answer. */
function run(plugin, args) {
  const candidates = plugin.settings.command ? [plugin.settings.command] : plugin.found ? [plugin.found] : COMMANDS;
  return new Promise((resolve, reject) => {
    const attempt = (index) => {
      execFile(candidates[index], args, { timeout: 15_000, maxBuffer: 16 * 1024 * 1024, windowsHide: true }, (error, stdout, stderr) => {
        if (error && error.code === "ENOENT") {
          if (index + 1 < candidates.length) return attempt(index + 1);
          return reject(new Error(NOT_INSTALLED));
        }
        if (!plugin.settings.command) plugin.found = candidates[index];
        if (error && error.code !== 1) return reject(new Error(`Ibis: ${firstLine(stderr) || error.message}`));
        resolve({ stdout, stderr });
      });
    };
    attempt(0);
  });
}

function firstLine(text) {
  return String(text || "").trim().replace(/^ibis: /, "").split("\n")[0];
}

/** Library options, as `ibis --file` and `--zotero` take them; none means the app's choice. */
function libraries(settings) {
  const args = [];
  for (const file of settings.file.split("\n").map((line) => line.trim()).filter(Boolean)) args.push("--file", file);
  if (settings.zotero) args.push("--zotero");
  return args;
}

/** One search hit as the modal shows it. */
function reference(hit) {
  const data = hit.item.object.data;
  const csl = data.csl || {};
  const names = (csl.author || csl.editor || []).map((name) => name.family || name.literal).filter(Boolean);
  const year = csl.issued && csl.issued["date-parts"] && csl.issued["date-parts"][0] && csl.issued["date-parts"][0][0];
  return {
    key: data.citation_keys && data.citation_keys[0] ? data.citation_keys[0].value : null,
    title: csl.title || "Untitled",
    details: [names.length > 2 ? `${names[0]} et al.` : names.join(" and "), year].filter(Boolean).join(", "),
  };
}

async function search(plugin, query) {
  const { stdout } = await run(plugin, ["search", query, "--json", "--limit", "20", ...libraries(plugin.settings)]);
  const answer = JSON.parse(stdout);
  if (!answer.ok) throw new Error(`Ibis: ${answer.error.code}`);
  return answer.data.hits.map(reference);
}

async function cite(plugin, key) {
  const { stdout, stderr } = await run(plugin, ["cite", key, "--for-app", APP_ID, ...libraries(plugin.settings)]);
  const text = stdout.trim();
  if (!text) throw new Error(`Ibis: ${firstLine(stderr) || "no citation for this reference"}`);
  return text;
}

class CiteModal extends SuggestModal {
  constructor(plugin, editor) {
    super(plugin.app);
    this.plugin = plugin;
    this.editor = editor;
    // Where the citation goes: the selection when the command ran, in the note as it was then.
    this.target = { from: editor.getCursor("from"), to: editor.getCursor("to"), text: editor.getValue() };
    this.asked = 0;
    this.newest = { asked: 0, found: [] };
    this.setPlaceholder("Search your references by author, title, year or key");
    this.emptyStateText = NO_MATCHES;
  }

  async getSuggestions(query) {
    // Obsidian shows whichever answer arrives last, so each answer shows the
    // current query's results: none while it is still being searched.
    const asked = ++this.asked;
    if (!query.trim()) {
      this.newest = { asked, found: [] };
      return [];
    }
    let found = [];
    let message = NO_MATCHES;
    try {
      found = await search(this.plugin, query);
    } catch (error) {
      message = error.message;
    }
    if (asked === this.asked) {
      this.newest = { asked, found };
      this.emptyStateText = message;
    }
    return this.newest.asked === this.asked ? this.newest.found : [];
  }

  renderSuggestion(item, el) {
    el.createEl("div", { text: item.title });
    el.createEl("small", { text: [item.details, item.key || "no citation key"].filter(Boolean).join(" · ") });
  }

  async onChooseSuggestion(item) {
    if (!item.key) return new Notice("Ibis: this reference has no citation key to cite with.");
    try {
      const text = await cite(this.plugin, item.key);
      const { from, to, text: note } = this.target;
      if (this.editor.getValue() !== note) return new Notice("Ibis: the note changed while citing, so nothing was inserted. Try again.");
      this.editor.replaceRange(text, from, to);
    } catch (error) {
      new Notice(error.message);
    }
  }
}

class IbisSettings extends PluginSettingTab {
  constructor(plugin) {
    super(plugin.app, plugin);
    this.plugin = plugin;
  }

  display() {
    const { containerEl } = this;
    const settings = this.plugin.settings;
    const save = (key) => async (value) => {
      settings[key] = value;
      this.plugin.found = null;
      await this.plugin.saveData(settings);
    };
    containerEl.empty();
    new Setting(containerEl)
      .setName("Library files")
      .setDesc("A .bib, CSL-JSON or RIS file to search, one path per line. Leave empty to use the libraries chosen in the Ibis app.")
      .addTextArea((text) => text.setPlaceholder("/path/to/references.bib").setValue(settings.file).onChange(save("file")));
    new Setting(containerEl)
      .setName("Zotero")
      .setDesc("Also search the local Zotero library, open or closed.")
      .addToggle((toggle) => toggle.setValue(settings.zotero).onChange(save("zotero")));
    new Setting(containerEl)
      .setName("ibis command")
      .setDesc("Leave empty to find it on PATH or where Ibis and Homebrew install it.")
      .addText((text) => text.setPlaceholder("ibis").setValue(settings.command).onChange(save("command")));
  }
}

module.exports = class IbisPlugin extends Plugin {
  async onload() {
    this.settings = Object.assign({}, DEFAULTS, await this.loadData());
    this.found = null;
    this.addCommand({
      id: "insert-citation",
      name: "Insert citation",
      editorCallback: (editor) => new CiteModal(this, editor).open(),
    });
    this.addSettingTab(new IbisSettings(this));
  }
};
