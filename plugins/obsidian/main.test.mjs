// Runs the plugin against the real `ibis` command, with a stand-in for the
// Obsidian API: `IBIS=target/debug/ibis node --test plugins/obsidian/`.
// crates/ibis-cli/tests/obsidian.rs runs it with the built command.
import assert from "node:assert/strict";
import Module, { createRequire } from "node:module";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const references = join(here, "../../fixtures/syntax-tools/references.bib");

class Modal {
  constructor(app) { this.app = app; }
  setPlaceholder() {}
  open() { this.opened = true; }
}
const notices = [];
const obsidian = {
  Plugin: class { constructor() { this.commands = []; } addCommand(c) { this.commands.push(c); } addSettingTab() {} async loadData() { return this.data; } },
  PluginSettingTab: class {},
  Setting: class {},
  SuggestModal: Modal,
  Notice: class { constructor(text) { notices.push(text); } },
};
const load = Module._load;
Module._load = function (request, ...rest) {
  return request === "obsidian" ? obsidian : load.call(this, request, ...rest);
};
const IbisPlugin = createRequire(import.meta.url)("./main.js");

async function modal(data) {
  const plugin = new IbisPlugin();
  plugin.data = { command: process.env.IBIS, ...data };
  await plugin.onload();
  const editor = { inserted: [], replaceSelection(text) { this.inserted.push(text); } };
  let opened;
  const open = Modal.prototype.open;
  Modal.prototype.open = function () { opened = this; };
  plugin.commands.find((c) => c.id === "insert-citation").editorCallback(editor);
  Modal.prototype.open = open;
  return { opened, editor };
}

test("searching and choosing inserts the citation in Obsidian's format", { skip: !process.env.IBIS && "IBIS is not set" }, async () => {
  const { opened, editor } = await modal({ file: references });
  const found = await opened.getSuggestions("synthetic signals");
  assert.deepEqual(found, [{ key: "taylor2026", title: "Synthetic signals", details: "Taylor and Dale, 2026" }]);
  await opened.onChooseSuggestion(found[0]);
  assert.deepEqual(editor.inserted, ["[@taylor2026]"]);
});

test("an older search never replaces a newer one", { skip: !process.env.IBIS && "IBIS is not set" }, async () => {
  const { opened } = await modal({ file: references });
  // Obsidian shows whichever answer arrives last.
  let shown;
  const show = (found) => { shown = found; };
  await Promise.all([opened.getSuggestions("invented").then(show), opened.getSuggestions("synthetic").then(show)]);
  assert.equal(shown[0].key, "taylor2026");
});

test("a missing command says how to install it", async () => {
  const { opened } = await modal({ command: join(here, "no-such-ibis") });
  assert.deepEqual(await opened.getSuggestions("anything"), []);
  assert.match(opened.emptyStateText, /ibis command was not found/);
  assert.deepEqual(notices, []);
});
