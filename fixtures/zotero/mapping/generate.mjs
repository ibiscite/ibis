// Synthetic fixture oracle for #1195. Run from any directory with Node 22+.
// Runs pinned, unmodified Zotero conversion code; never opens Zotero or a library.
// Adapter for schema lookups only. Dates use en-US and UTC for reproducibility.
// The whitespace and padding helpers mirror zotero/utilities (AGPL-3.0-or-later),
// Copyright © Center for History and New Media / Corporation for Digital Scholarship.
import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import vm from "node:vm";

const here = path.dirname(fileURLToPath(import.meta.url));
const vendor = path.resolve(here, "../../../crates/ibis-sources/vendor/zotero");
const manifest = JSON.parse(await readFile(path.join(vendor, "manifest.json"), "utf8"));
process.env.TZ = manifest.oracleProfile.timeZone;
for (const { out, sha256 } of manifest.files) {
  const bytes = await readFile(path.join(vendor, out));
  if (createHash("sha256").update(bytes).digest("hex") !== sha256) {
    throw new Error(`Pinned oracle digest changed: ${out}`);
  }
}
const schema = JSON.parse(await readFile(path.join(vendor, "schema.json"), "utf8"));
const types = new Map(schema.itemTypes.map(type => [type.itemType, type]));
const fieldNames = new Set(schema.itemTypes.flatMap(type => type.fields.flatMap(field => [field.field, field.baseField].filter(Boolean))));
let currentItem;
const zotero = {
  locale: manifest.oracleProfile.locale, isClient: manifest.oracleProfile.isClient, debug() {},
  getString(key) { if (key === "date.daySuffixes") return manifest.oracleProfile.strings[key].value; throw new Error(`Unexpected string: ${key}`); },
  Utilities: {
    XRegExp(pattern, flags) { return new RegExp(pattern, flags); },
    trimInternal(value) { return value.replace(/[\xA0\r\n\s]+/g, " ").trim(); },
    lpad(value, pad, length) { let result = value ? String(value) : ""; while (result.length < length) result = pad + result; return result; },
  },
  ItemTypes: { getID: value => types.has(value) ? value : false, getName: value => value },
  Items: { get() { return currentItem; } },
  Prefs: { get(key) { if (key === "export.citePaperJournalArticleURL") return false; throw new Error(`Unexpected preference: ${key}`); } },
  ItemFields: {
    getID: value => fieldNames.has(value) ? value : false,
    getName: value => value,
    getFieldIDFromTypeAndBase(type, field) {
      return types.get(type)?.fields.find(candidate => candidate.baseField === field)?.field || false;
    },
  },
  CreatorTypes: {
    getName: value => value,
    getPrimaryIDForType: type => types.get(type)?.creatorTypes.find(creator => creator.primary)?.creatorType || false,
  },
};
const context = vm.createContext({ Zotero: zotero, console, Date });
for (const file of ["schema.js", "date.js", "utilities_item.js", "cite.js"]) {
  vm.runInContext(await readFile(path.join(vendor, file), "utf8"), context, { filename: file });
}
zotero.Schema.init(schema);
zotero.Date.init(JSON.parse(await readFile(path.join(vendor, "dateFormats.json"), "utf8")));
const system = new zotero.Cite.System({});
const cases = [];
let serial = 0;
function add(name, fields) {
  const item = { key: `S${String(++serial).padStart(7, "0")}`, itemType: "book", ...fields };
  currentItem = { ...item, id: serial, itemTypeID: item.itemType, getField(field) { return item[field] || ""; } };
  const output = system.retrieveItem(serial);
  delete output.id;
  cases.push({ name, item, expected: output });
}
const dateFields = new Set(["date", "filingDate", "originalDate", "dateDecided", "issueDate", "dateEnacted"]);
for (const type of schema.itemTypes) {
  if (["annotation", "attachment", "note"].includes(type.itemType)) continue;
  const item = { itemType: type.itemType };
  for (const { field, baseField } of type.fields) {
    if (dateFields.has(field) || dateFields.has(baseField)) item[field] = "2024-02-29";
    else if (field === "accessDate") item[field] = "2026-10-25 23:45:01";
    else if (field === "extra") item[field] = "Invented free text\nOriginal Title: Invented earlier title\nIssued: 2021/2023";
    else if (field === "url") item[field] = "https://example.invalid/invented";
    else if (field === "ISBN") item[field] = "978-0-00-000000-0 978-0-00-000001-7";
    else if (field === "ISSN") item[field] = "0000-0000";
    else if (field === "DOI") item[field] = "10.0000/invented";
    else if (field === "language") item[field] = "en-US";
    else if (field === "citationKey") item[field] = "Synthetic2026";
    else item[field] = `Invented ${field}`;
  }
  item.creators = type.creatorTypes.flatMap(({ creatorType }, index) => [
    { creatorType, firstName: `Example${index}`, lastName: "Synthetic" },
    { creatorType, name: `Invented ${creatorType} collective` },
  ]);
  add(`type-${type.itemType}`, item);
}
for (const date of [
  "2026", "2026-10", "2024-02-29", "2026-02-30", "2026-13-01", "2026-00-00", "2026-01-00",
  "2020/2026", "2020-01/2026-10-25", "1995-96", "2021-22", "2020–2026", "2020--2026", "2026/2020",
  "2026?", "2026-10~", "2020?/2026%", "circa 2026", "ca. 2026", "~2026", "around 2026",
  "429 BCE", "1000-900 BCE", "100 BCE-50 CE", "AD 429", "-0429", "-429", "0000", "0001", "429", "0 BCE",
  '"Invented date"', "“Invented date”", "undated", "Summer 2026", "January 10, 2026", "10 January 2026",
  "January 10 200 BCE", "2026年10月25日", "10/25/2026", "25/10/2026", "2026.10.25", "10/2026", "2026-10-25T23:45:01Z",
]) add(`date-${date}`, { title: "Invented date conversion", date });
for (const extra of [
  "DOI: 10.0000/invented\nPublication Title: Invented journal\nOriginal Date: 1901-02-03",
  "Original Date: 1901/1903\nIssued: 2001?\nEvent Date: circa 2026\nSubmitted: -0429",
  'Issued: "Invented literal"\nOriginal Date: “Earlier invented literal”',
  "Invented first free line\nIssued: 2001/2003\nInvented interruption\nIssued: 2010/2012",
  "Issued: 2020/2021\nIssued: 2022/2023\noriginal-date: 1800",
  "AUTHOR: Synthetic || Example\nTitle: Invented title\nURL: https://example.invalid/extra",
  "Archive Location: Invented archive\nArchive-Location: Invented second archive\nAbstract Note: Invented abstract",
  "Citation Key: Invented2026\ntex.ids= Earlier2020, Earlier2021\nIssued: 2001/2003",
  "Unknown Field: Invented private marker\nISSN: 0000-0000\nPMID: 00000000\nPMCID: PMC0000000",
  "Title: \n Issued: 2001/2003\nissued: 2020-00-00",
  "Original Date: 1900-01-01\r\nIssued: 2001/2003\r\nPublisher: Invented publisher",
]) add(`extra-${cases.length}`, { title: "Invented Extra conversion", date: "2026", extra });
for (const [firstName, lastName] of [
  ["Example", "de Synthetic"], ["Example", "van der Synthetic"], ["Example", "d’Synthetic"],
  ["Example", "d' Synthetic"], ["Example", '"de Synthetic"'], ["Example de", "Synthetic"],
  ["Example, Jr.", "Synthetic"], ["Example,, Jr.", "Synthetic"], ["Example, et al.", "Synthetic"],
  ["Example d’", "Synthetic"], ["Example", "von und zu Synthetic"], ["", "Synthetic"], ["Example", ""],
  ["日本語", "合成"], ["مخترع", "اصطناعي"],
]) add(`creator-${cases.length}`, { creators: [{ creatorType: "author", firstName, lastName }] });
add("creator-field-mode", { creators: [{ creatorType: "author", fieldMode: 1, lastName: "Invented collective" }] });
add("creator-mixed-order", { creators: [{ creatorType: "editor", name: "Invented editors" }, { creatorType: "author", name: "Invented authors" }, { creatorType: "author", firstName: "Example", lastName: "Synthetic" }] });
for (const itemType of ["journalArticle", "newspaperArticle", "magazineArticle", "book", "preprint"]) {
  for (const pages of ["1–9", ""]) add(`url-${itemType}-${pages || "absent"}`, { itemType, title: "Invented article", pages, url: "https://example.invalid/print", accessDate: "2026-10-25 23:45:01" });
}
for (const accessDate of ["2026-10-25", "2026-10-25T23:45:01Z", "2026-10-25 23:45:01", "2026-10-25T23:45:01+02:00"]) {
  add(`access-${accessDate}`, { title: "Invented access date", accessDate });
}
for (const ISBN of ["978-0-00-000000-0 978-0-00-000001-7", "000000000X 0000000000", "Invented invalid ISBN", '"9780000000000"']) add(`isbn-${cases.length}`, { ISBN });
add("quoted-text", { title: '"Invented title"', publisher: '"Invented "inner" quotes"', shortTitle: '"Invented short"' });
for (const [index, extra] of [
  "", " ", "\t", "\n", "\r", "\r\n", "\n\nIssued: 2001/2003\n",
  "Title:\nIssued: 2001/2003", "Title: \t\n\nIssued: 2001/2003", "Title:\rIssued: 2001/2003",
  "Title:\u2028Issued: 2001/2003", "Title:\u2029Issued: 2001/2003",
  "Title: \u00a0\r\nIssued: 2001/2003", "\rIssued: 2001/2003\rOriginal Date: 1800/1801",
  "Issued: 2001/2003\u2028Original Date: 1800/1801", "Issued: 2001/2003\u2029Original Date: 1800/1801",
  "{:issued: 2001/2003}", "Invented prefix {:original-title: Invented embedded title}",
  "Title: Invented title\n{:issued: 2001/2003}\nOriginal Date: 1800/1801",
  "Invented first line\n\t\nIssued: 2001/2003", "Title:\n\n\nIssued: 2001/2003",
  "Type: article-journal\nIssued: 2001/2003", "original_date: 1800/1801\nEVENT DATE: 2001/2003",
  "Original  Date: 1800/1801\nOriginal-Date: 1802/1803", "  Issued: 2001/2003\nOriginal Date: 1800/1801",
  "Issued: 2001/2003}\nOriginal Date: 1800/1801", "Issued:\nOriginal Date: 1800/1801",
  'Issued: ""\nOriginal Date: "Invented literal"', "Issued: 2001/2003\n\r\nOriginal Date: 1800/1801",
].entries()) add(`extra-edge-${index}`, { title: "Invented Extra edge", date: "2026", extra });
for (const [index, [firstName, lastName]] of [
  ["Example", "deSynthetic"], ["Example", "de-Synthetic"], ["Example", "de–Synthetic"],
  ["Example", "D'Synthetic"], ["Example", "d'Synthetic"], ["Example", "dʼSynthetic"], ["Example", "ʻSynthetic"],
  ["Example", "DE Synthetic"], ["Example", "De Synthetic"], ["Example", "de  Synthetic"],
  ["Example", "de\tSynthetic"], ["Example", "de\u00a0Synthetic"], ["Example", "de 'Synthetic"],
  ["Example", "'de Synthetic"], ["Example", "de' Synthetic"], ["Example", "d’ Synthetic"],
  ["Example", "de - ' Synthetic"], ["Example", "van der  Synthetic"], ["Example", "de"],
  ["Example de", "de Synthetic"], ["Example  de", "Synthetic"], ["Example de la", "Synthetic"],
  ["Example d'", "Synthetic"], ["Example d' ", "Synthetic"], ["Example D'", "Synthetic"],
  ["Example , Jr.", "Synthetic"], ["Example,! Jr.", "Synthetic"], ["Example,!! Jr.", "Synthetic"],
  ["Example,,Jr.", "Synthetic"], ["Example, et al", "Synthetic"], ["Example, et al. de", "Synthetic"],
  ["Example, Jr., III", "Synthetic"], ["Example\u00a0de", "Synthetic"], ["Example", "«de Synthetic»"],
  ["Example", '"de Synthetic'], ["Example", 'de Synthetic"'], ["E", "\u0301de Synthetic"],
].entries()) add(`creator-edge-${index}`, { creators: [{ creatorType: "author", firstName, lastName }] });
for (const [index, ISBN] of [
  "9780000000000x", "9780000000000-x", "9780000000000_", "9780000000000日本", "9780000000000😀",
  "9780000000000\u00a0Invented", "000000000x", "000000000X-x", "0000000000-", "00000000000",
  "9780000000000. 9780000000017", " 9780000000000", "\u00a09780000000000", "９７８００００００００００",
  "978-0-00-000000-0—978-0-00-000001-7", "978--0000000000", "9790000000000", "978000000000x",
].entries()) add(`isbn-edge-${index}`, { ISBN });
for (const date of [
  "January 3rd, 2026", "May 1st 2026", "22nd May 2026", "AD 200 January 10", "January AD 200 10",
  "January 10 AD 200", "10 January 200 B.C.E.", "January 12345", "January 00000", "Invented 12345 date",
  "2000-02-29", "1900-02-29", "2024-02-30", "2026-04-31", "2026-10-25\u00a0", "0000 BCE",
]) add(`date-edge-${date}`, { date });
for (const accessDate of [
  "2026-10-25T00:15:01+02:00", "2026-10-25T23:45:01-02:00", "2026-01-01T00:15:01+14:00",
  "2026-12-31T23:45:01-12:00", "2026-10-25T00:15:01+0230", "2026-10-25 00:15",
]) add(`access-edge-${accessDate}`, { accessDate });
add("base-collision-conference", { itemType: "conferencePaper", title: "Invented title", publicationTitle: "Invented base publication", proceedingsTitle: "Invented type publication", date: "", place: "", publisher: "" });
add("base-collision-case", { itemType: "case", title: "Invented base title", caseName: "Invented case name", date: "", dateDecided: "2001/2003", number: "Invented base number", docketNumber: "Invented docket" });
add("base-collision-program", { itemType: "computerProgram", title: "Invented title", versionNumber: "", version: "Invented old version", company: "", publisher: "Invented base publisher" });
add("base-collision-presentation", { itemType: "presentation", title: "", date: "", place: "Invented original place", eventPlace: "Invented new place", presentationType: "", type: "Invented base type" });
add("empty-text-fields", { title: "", abstractNote: "", date: "", citationKey: "", url: "", accessDate: "", extra: "", shortTitle: "", ISBN: "", creators: [] });
for (const [index, accessDate] of [
  "2026-10-25 nonsense", "2026-10-25T99:99:99Z", "2026-10-25TbadZ",
  "2026-10-25 99:99:99", "2026-10-25 12:34:56Z",
].entries()) add(`access-malformed-tail-${index}`, { accessDate });
for (const [index, separator] of ["\n", "\r", "\u2028", "\u2029"].entries()) {
  add(`creator-family-line-${index}`, { creators: [{ creatorType: "author", firstName: "Example", lastName: `van de${separator}Synthetic` }] });
  add(`creator-family-leading-line-${index}`, { creators: [{ creatorType: "author", firstName: "Example", lastName: `van${separator}de Synthetic` }] });
  add(`creator-given-line-${index}`, { creators: [{ creatorType: "author", firstName: `Example${separator}van de`, lastName: "Synthetic" }] });
  add(`creator-given-particle-line-${index}`, { creators: [{ creatorType: "author", firstName: `Example van${separator}de`, lastName: "Synthetic" }] });
}
for (const date of ["May 3rd, 2026", "200 BC Spring", "200 BCE January 3", "5/3/00"]) {
  add(`date-profile-${date}`, { date });
}
for (const accessDate of ["2026-10", "2026"]) add(`access-short-${accessDate}`, { accessDate });
for (const url of ["www.example.invalid/invented", "doi.org/10.0000/invented", "example.invalid/invented"]) {
  add(`url-schemeless-${url}`, { url });
}
for (const accessDate of [
  "2026-02-30", "2026-02-30T00:00:00Z", "2026-02-30 12:00:00",
  "0001", "0001-01-01", "0001-01-01T00:00:00Z", "0001-01-01 00:00:00",
]) add(`access-calendar-${accessDate}`, { accessDate });
add("extra-greedy-blank-publisher", { date: "2026", extra: "Title: Invented title\nPublisher: \nissued: 2001/2003" });
for (const date of ["27 BC–AD 14", "March 15 44BC", "Spring 200BC"]) add(`date-era-review-${date}`, { date });
add("extra-era-review", { date: "2026", extra: "Issued: 27 BC–AD 14" });
for (const [index, separator] of ["\n", "\r", "\u2028", "\u2029"].entries()) {
  add(`creator-uppercase-family-line-${index}`, { creators: [{ creatorType: "author", firstName: "Example", lastName: `Van${separator}der Synthetic` }] });
  add(`creator-uppercase-given-line-${index}`, { creators: [{ creatorType: "author", firstName: `Example Van${separator}der`, lastName: "Synthetic" }] });
}
for (const [index, separator] of ["\u2028", "\u2029"].entries()) {
  add(`date-quoted-line-${index}`, { date: `"Invented${separator}date"` });
  add(`extra-quoted-line-${index}`, { date: "2026", extra: `Issued: "Invented${separator}date"` });
}
const output = `${JSON.stringify(cases, null, 2)}\n`;
const outputPath = path.join(here, "cases.json");
if (process.argv.includes("--check")) {
  if (await readFile(outputPath, "utf8") !== output) throw new Error("Zotero differential fixture drift");
} else {
  await writeFile(outputPath, output);
}
console.log(`${cases.length} invented Zotero mapping fixtures; ${schema.itemTypes.length - 3} bibliographic item types`);
