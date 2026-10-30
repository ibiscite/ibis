// Weekly, aggregate GitHub counters only; no app data or identifiers.
// .dmg/.msi/.deb/.rpm suggest new downloads; .app.tar.gz suggests updates.
// Windows setup .exe and Linux AppImage mix both; keep raw names for readers.
import { fileURLToPath } from "node:url";

const ref = "refs/heads/stats";
const header = "date,version,asset_name,download_count\n";
const hash = (value) => typeof value === "string" && value.length === 40 && /^[a-f0-9]{40}$/.test(value);
const positive = (value) => Number.isSafeInteger(value) && value > 0;
const version = (value) => typeof value === "string" && !/\s/.test(value)
  && /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(value);
const require = (condition, message) => { if (!condition) throw new Error(message); };

function week(date) {
  const day = new Date(`${date}T00:00:00Z`);
  require(!Number.isNaN(day.valueOf()) && day.toISOString().slice(0, 10) === date, "Invalid CSV date.");
  day.setUTCDate(day.getUTCDate() - (day.getUTCDay() + 6) % 7);
  return day.toISOString().slice(0, 10);
}

// Strict CSV parsing is needed for weekly deduplication even when names contain
// line breaks. Never repair/rewrite existing bytes or guess at damaged history.
function parseCsv(text) {
  require(text.startsWith(header) && text.endsWith("\n"), "Invalid counts.csv header or ending.");
  const rows = [];
  let row = [], field = "", state = "start";
  for (const char of text.slice(header.length)) {
    if (state === "quoted") {
      if (char === '"') state = "closed";
      else field += char;
    } else if (state === "closed" && char === '"') {
      field += '"'; state = "quoted";
    } else if (char === "," || char === "\n") {
      row.push(field); field = ""; state = "start";
      if (char === "\n") { rows.push(row); row = []; }
    } else if (state === "start" && char === '"') {
      state = "quoted";
    } else {
      require(state !== "closed" && char !== '"' && char !== "\r", "Invalid CSV quoting.");
      field += char; state = "plain";
    }
  }
  require(state === "start" && row.length === 0, "Incomplete CSV row.");
  for (const fields of rows) {
    require(fields.length === 4 && /^\d{4}-\d{2}-\d{2}$/.test(fields[0]) && version(fields[1])
      && fields[2].length > 0 && !fields[2].includes("\0") && /^\d+$/.test(fields[3])
      && Number.isSafeInteger(Number(fields[3])) && String(Number(fields[3])) === fields[3], "Invalid counts.csv row.");
    week(fields[0]);
  }
  return rows;
}

const quote = (value) => /[",\r\n]/.test(value) ? `"${value.replaceAll('"', '""')}"` : value;

export async function recordCounts({ repository, token, now = new Date(), fetchImpl = fetch }) {
  require(typeof repository === "string" && /^[A-Za-z0-9_-][A-Za-z0-9_.-]*\/[A-Za-z0-9_-][A-Za-z0-9_.-]*$/.test(repository)
    && !/\s/.test(repository) && typeof token === "string" && token.length > 0, "Missing or invalid GitHub repository/token.");
  const date = now.toISOString().slice(0, 10);
  const currentWeek = week(date);
  const api = async (method, path, body, { missing = false, asset = false } = {}) => {
    const response = await fetchImpl(`https://api.github.com/repos/${repository}/${path}`, {
      method,
      headers: {
        Authorization: `Bearer ${token}`,
        Accept: asset ? "application/octet-stream" : "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        ...(body ? { "Content-Type": "application/json" } : {}),
      },
      // GitHub may redirect an asset to its download host; Fetch strips
      // Authorization on cross-origin redirects. JSON API reads never redirect.
      redirect: asset ? "follow" : "error",
      signal: AbortSignal.timeout(30_000),
      ...(body ? { body: JSON.stringify(body) } : {}),
    });
    if (missing && response.status === 404) return undefined;
    require(response.ok, `GitHub API ${method} failed (${response.status}).`);
    return response.json();
  };
  const objectHash = (object) => {
    require(hash(object?.sha), "Invalid Git object response.");
    return object.sha;
  };

  const release = await api("GET", "releases/tags/stable");
  require(positive(release?.id) && release.tag_name === "stable" && release.draft === false, "Invalid stable release response.");
  const assets = [];
  const ids = new Set(), names = new Set();
  for (let page = 1; ; page++) {
    const batch = await api("GET", `releases/${release.id}/assets?per_page=100&page=${page}`);
    require(Array.isArray(batch) && batch.length <= 100, "Invalid release assets response.");
    for (const asset of batch) {
      require(positive(asset?.id) && typeof asset.name === "string" && asset.name.length > 0
        && !asset.name.includes("\0") && Number.isSafeInteger(asset.download_count) && asset.download_count >= 0
        && !ids.has(asset.id) && !names.has(asset.name), "Invalid or duplicate release asset.");
      ids.add(asset.id); names.add(asset.name); assets.push(asset);
    }
    if (batch.length < 100) break;
  }
  require(assets.length > 0, "Stable release has no assets.");
  const latest = assets.find((asset) => asset.name === "latest.json");
  const releaseVersion = latest
    ? (await api("GET", `releases/assets/${latest.id}`, undefined, { asset: true }))?.version
    : /^Ibis v?(\S+)$/.exec(release.name ?? "")?.[1];
  require(version(releaseVersion), "Stable release has no valid version.");

  const previous = await api("GET", "git/ref/heads/stats", undefined, { missing: true });
  let csv = header, parent = null, baseTree;
  if (previous !== undefined) {
    require(previous?.ref === ref && previous.object?.type === "commit" && hash(previous.object.sha), "Refusing a non-stats commit ref.");
    parent = previous.object.sha;
    const commit = await api("GET", `git/commits/${parent}`);
    require(objectHash(commit) === parent && hash(commit.tree?.sha) && Array.isArray(commit.parents)
      && commit.parents.length <= 1 && commit.parents.every((p) => hash(p?.sha)), "Invalid stats commit.");
    baseTree = commit.tree.sha;
    const tree = await api("GET", `git/trees/${baseTree}`);
    require(objectHash(tree) === baseTree && tree.truncated === false && Array.isArray(tree.tree) && tree.tree.length === 1,
      "Refusing a stats tree containing anything except counts.csv.");
    const entry = tree.tree[0];
    require(entry.path === "counts.csv" && entry.type === "blob" && entry.mode === "100644" && hash(entry.sha), "Invalid stats CSV entry.");
    const blob = await api("GET", `git/blobs/${entry.sha}`);
    require(objectHash(blob) === entry.sha && blob.encoding === "base64" && typeof blob.content === "string", "Invalid stats CSV blob.");
    const encoded = blob.content.replaceAll("\n", "");
    const bytes = Buffer.from(encoded, "base64");
    csv = bytes.toString("utf8");
    require(bytes.toString("base64") === encoded && Buffer.from(csv).equals(bytes), "Invalid stats CSV encoding.");
  }
  const rows = parseCsv(csv);
  const weeks = rows.map(([date]) => week(date));
  require(weeks.every((past) => past <= currentWeek), "Refusing a snapshot older than recorded history.");
  if (weeks.includes(currentWeek)) return "This UTC week is already recorded.";
  const appended = assets.map((asset) => [date, releaseVersion, asset.name, String(asset.download_count)].map(quote).join(",") + "\n").join("");

  // Objects alone do not publish anything. Only the last request moves the
  // fixed ref, without force; a racing append fails instead of losing rows.
  const blob = objectHash(await api("POST", "git/blobs", { content: csv + appended, encoding: "utf-8" }));
  const tree = objectHash(await api("POST", "git/trees", {
    ...(baseTree ? { base_tree: baseTree } : {}),
    tree: [{ path: "counts.csv", mode: "100644", type: "blob", sha: blob }],
  }));
  const commit = objectHash(await api("POST", "git/commits", {
    message: `chore: record release counts (${date})`, tree, parents: parent ? [parent] : [],
  }));
  const updated = parent
    ? await api("PATCH", "git/refs/heads/stats", { sha: commit, force: false })
    : await api("POST", "git/refs", { ref, sha: commit });
  require(updated?.ref === ref && updated.object?.type === "commit" && updated.object.sha === commit, "Invalid updated stats ref.");
  return `Recorded ${assets.length} stable release assets for ${date}.`;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    require(process.argv.length === 2 && process.env.GITHUB_REF === "refs/heads/main"
      && ["schedule", "workflow_dispatch"].includes(process.env.GITHUB_EVENT_NAME), "Run this workflow from main by schedule or workflow_dispatch.");
    console.log(await recordCounts({ repository: process.env.GITHUB_REPOSITORY, token: process.env.GH_TOKEN }));
  } catch (error) {
    // API bodies, asset names, URLs and credentials never enter Actions logs.
    console.error(error instanceof SyntaxError ? "Invalid GitHub JSON response." : error instanceof TypeError ? "GitHub request failed." : error.message);
    process.exitCode = 1;
  }
}
