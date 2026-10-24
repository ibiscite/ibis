import { readFile } from "node:fs/promises";
const text = path => readFile(new URL(path, import.meta.url), "utf8");
const escape = value => value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;");
export async function content(html) {
  const headline = JSON.parse(await text("../brand/settings.json")).words.tagline;
  const design = (await text("../docs/design.md")).split("## Coming features")[1]?.split("## How the design changes")[0];
  const roadmap = await text("../ROADMAP.md");
  if (!design) throw Error("Canonical Coming features section is missing");
  const rows = [...design.matchAll(/^\| ([^|]+) \|/gm)].map(([, feature]) => feature);
  const plans = ["Page numbers and locators", "Zotero while closed", "Insertion"].map(name => {
    const row = rows.find(value => value.startsWith(name));
    const version = row?.match(/\((0\.\d)/)?.[1];
    if (!row || !version || !roadmap.includes(`**${version} `)) throw Error(`Canonical roadmap item missing: ${name}`);
    const label = row.replace(/\s*\([^)]*\)/g, "");
    return `<li><strong>${escape(label)}</strong><span>Planned for ${version}</span></li>`;
  });
  return html.replaceAll("{{headline}}", escape(headline)).replace("{{roadmap}}", `<ul class="roadmap">${plans.join("")}</ul>`);
}
