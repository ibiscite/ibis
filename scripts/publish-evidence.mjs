// Only synthetic screenshots are published. The destination ref is fixed;
// GitHub's default token is repository-scoped, not branch-scoped.
import { spawnSync } from "node:child_process";
import { copyFile, lstat, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const commit = /^[a-f0-9]{40}$/;
const platforms = ["Linux", "macOS", "Windows"];

async function existing(path, kind) {
  let stat;
  try { stat = await lstat(path); }
  catch (error) { if (error.code === "ENOENT") return false; throw error; }
  if (stat.isSymbolicLink()) throw new Error("Evidence destination contains a symbolic link");
  if (kind === "directory" ? !stat.isDirectory() : !stat.isFile()) throw new Error("Invalid evidence destination type");
  return true;
}

async function captures(source, relative = "", os) {
  const files = [];
  for (const entry of await readdir(join(source, relative), { withFileTypes: true })) {
    const name = join(relative, entry.name);
    if (entry.isSymbolicLink()) throw new Error("Capture contains a symbolic link");
    if (os && [...entry.name.matchAll(/-(Linux|macOS|Windows)(?=\.|-|$)/g)].some(match => match[1] !== os)) {
      throw new Error("Capture contains mixed platforms");
    }
    if (entry.isDirectory()) files.push(...await captures(source, name, os));
    else if (entry.isFile() && /\.(png|jpe?g|webp)$/i.test(entry.name)) files.push(name);
    else if (!entry.isFile()) throw new Error("Invalid capture entry type");
  }
  return files;
}

export async function evidenceSources({ input, sha }) {
  if (!commit.test(sha)) throw new Error("Expected a full commit SHA");
  if (!await existing(input, "directory")) return [];
  const entries = await readdir(input, { withFileTypes: true });
  const markers = [], sources = [];
  for (const entry of entries) {
    if (entry.isSymbolicLink()) throw new Error("Capture contains a symbolic link");
    const marker = platforms.find(os => entry.name === `picker-${os}.png`);
    const artifact = platforms.find(os => entry.name === `picker-${sha}-${os}`);
    if (marker && entry.isFile()) markers.push(marker);
    else if (artifact && entry.isDirectory()) sources.push({ source: resolve(input, entry.name), os: artifact });
    else if (entry.name.startsWith("picker-") || !entry.isFile() && !entry.isDirectory() || /\.(png|jpe?g|webp)$/i.test(entry.name)) {
      throw new Error("Invalid capture layout");
    }
  }
  if (markers.length > 1 || markers.length && sources.length) throw new Error("Ambiguous capture layout");
  // download-artifact extracts a singleton directly into its destination even
  // with merge-multiple:false. Infer its OS only from one fixed root marker.
  if (markers.length === 1) sources.push({ source: resolve(input), os: markers[0] });
  else if (sources.length && entries.some(entry => entry.isDirectory() && !sources.some(item => item.source === resolve(input, entry.name)))) {
    throw new Error("Mixed capture layouts");
  }
  // Validate every selected tree before the first OS destination is changed.
  for (const { source, os } of sources) await captures(source, "", os);
  return sources;
}

export async function prepareEvidence({ root, source, sha, os, now = Date.now() }) {
  if (!commit.test(sha)) throw new Error("Expected a full commit SHA");
  if (!platforms.includes(os)) throw new Error("Unknown capture platform");
  if ((await lstat(source)).isSymbolicLink()) throw new Error("Capture root is a symbolic link");
  const files = await captures(source, "", os);
  if (files.length === 0) return false;
  await existing(root, "directory");
  const indexPath = join(root, "index.json");
  const index = await existing(indexPath, "file") ? JSON.parse(await readFile(indexPath, "utf8")) : [];
  if (!Array.isArray(index) || index.some(row => !commit.test(row.sha) || !Number.isFinite(row.firstSeen))) {
    throw new Error("Invalid evidence index; refusing to prune");
  }
  // Validate existing ancestors before recursive removal or file creation.
  for (const row of index) await existing(join(root, row.sha), "directory");
  await existing(join(root, sha), "directory");
  const target = join(root, sha, os);
  await existing(target, "directory");
  await rm(target, { recursive: true, force: true });
  for (const file of files) {
    await mkdir(dirname(join(target, file)), { recursive: true });
    await copyFile(join(source, file), join(target, file));
  }
  if (!index.some(row => row.sha === sha)) index.push({ sha, firstSeen: now });
  index.sort((a, b) => a.firstSeen - b.firstSeen || a.sha.localeCompare(b.sha));
  for (const row of index.slice(0, -50)) await rm(join(root, row.sha), { recursive: true, force: true });
  await writeFile(indexPath, JSON.stringify(index.slice(-50), null, 2) + "\n");
  return true;
}

async function publish() {
  const { GITHUB_REPOSITORY: repository, GITHUB_TOKEN: token, EVIDENCE_SHA: sha, EVIDENCE_INPUT: input } = process.env;
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository ?? "") || !token || !commit.test(sha ?? "") || !input) {
    throw new Error("Missing trusted publication context");
  }
  // No credential is written to disk or embedded in command arguments.
  const env = { ...process.env, GIT_CONFIG_COUNT: "1",
    GIT_CONFIG_KEY_0: "http.https://github.com/.extraheader",
    GIT_CONFIG_VALUE_0: `AUTHORIZATION: basic ${Buffer.from(`x-access-token:${token}`).toString("base64")}`,
    GIT_AUTHOR_NAME: "github-actions[bot]", GIT_COMMITTER_NAME: "github-actions[bot]",
    GIT_AUTHOR_EMAIL: "41898282+github-actions[bot]@users.noreply.github.com",
    GIT_COMMITTER_EMAIL: "41898282+github-actions[bot]@users.noreply.github.com" };
  // Retry a concurrent publisher against its updated parent; never force push.
  for (let attempt = 0; attempt < 5; attempt++) {
    const directory = await mkdtemp(join(tmpdir(), "ibis-evidence-publish-"));
    const git = (...args) => {
      const result = spawnSync("git", args, { cwd: directory, env, encoding: "utf8" });
      if (result.error || result.status !== 0) throw new Error(result.stderr || "Evidence git command failed");
      return result.stdout.trim();
    };
    try {
      git("init", "-q");
      git("remote", "add", "origin", `https://github.com/${repository}.git`);
      if (git("ls-remote", "--heads", "origin", "refs/heads/evidence")) {
        git("fetch", "--depth=1", "origin", "refs/heads/evidence");
        git("checkout", "-q", "-B", "evidence", "FETCH_HEAD");
      } else git("checkout", "-q", "--orphan", "evidence");
      let changed = false;
      for (const { source, os } of await evidenceSources({ input, sha })) {
        try { changed = await prepareEvidence({ root: directory, source, sha, os }) || changed; }
        catch (error) { if (error.code !== "ENOENT") throw error; }
      }
      if (!changed) throw new Error("No platform screenshots to publish");
      git("add", "--all");
      const diff = spawnSync("git", ["diff", "--cached", "--quiet"], { cwd: directory, env });
      if (diff.status === 0) return;
      if (diff.status !== 1) throw new Error("Cannot compare evidence changes");
      git("commit", "-q", "-m", `ci: record native picker evidence for ${sha}`);
      git("push", "origin", "HEAD:refs/heads/evidence");
      console.log(`Native screenshots: ${repository}/tree/evidence/${sha}`);
      return;
    } catch (error) {
      if (attempt === 4 || !/non-fast-forward|fetch first|cannot lock ref|remote rejected/.test(error.message)) throw error;
    } finally { await rm(directory, { recursive: true, force: true }); }
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await publish();
}
