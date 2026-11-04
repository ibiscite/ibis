// Synthetic PDFs for the reader spike (#1252). Invented text only. Each file
// prints its own page numbers on the page, so a picture shows whether the
// PDF's /PageLabels agree with what a reader sees, and where they cannot.

/** A page-label range as written in /PageLabels: physical start (0-based). */
export type LabelRange = { start: number; style?: "D" | "r" | "R" | "a" | "A" | "X"; prefix?: string; first?: number };

export type SyntheticSpec = {
  name: string;
  title: string;
  /** What each physical page prints at its foot; null prints nothing. */
  printed: (string | null)[];
  /** Omitted: the file has no /PageLabels at all. */
  labels?: LabelRange[];
};

/** Front matter i–iv, a chapter offprint 413–416, an unnumbered plate, then
 * 419–421: the printed sequence skips 417–418 (pages removed from the scan). */
export const LABELLED: SyntheticSpec = {
  name: "labelled.pdf",
  title: "An Invented Monograph on Garden Walls",
  printed: ["i", "ii", "iii", "iv", "413", "414", "415", "416", "Plate 1", "419", "420", "421"],
  labels: [
    { start: 0, style: "r" },
    { start: 4, style: "D", first: 413 },
    { start: 8, style: "D", prefix: "Plate " },
    { start: 9, style: "D", first: 419 },
  ],
};

/** Numbers printed on the page but no /PageLabels: Ibis must not guess 27. */
export const UNLABELLED: SyntheticSpec = {
  name: "unlabelled.pdf",
  title: "An Invented Article Without Page Labels",
  printed: ["27", "28", "29"],
};

/** A label range that does not start at physical page 0, and one label used
 * twice: the first two pages get empty labels and "5" is ambiguous. */
export const PARTIAL: SyntheticSpec = {
  name: "partial.pdf",
  title: "An Invented Report With Partial Labels",
  printed: [null, null, "5", "6", "5"],
  labels: [{ start: 2, style: "D", first: 5 }, { start: 4, style: "D", first: 5 }],
};

/** An invalid style: PDF.js drops all labels for the file rather than guess. */
export const MALFORMED: SyntheticSpec = {
  name: "malformed.pdf",
  title: "An Invented Note With Broken Labels",
  printed: ["1", "2"],
  labels: [{ start: 0, style: "X" }],
};

export const SPECS = [LABELLED, UNLABELLED, PARTIAL, MALFORMED] as const;

const BODY = [
  "The walls of the invented garden were built in three seasons,",
  "each course of stone laid by a different and imaginary mason.",
  "Nothing in this passage describes a real place or a real text;",
  "it exists so that a reader can select, search and hear it.",
  "A quotation worth citing sits on this line, at a known place.",
];

const escape = (text: string) => text.replace(/[\\()]/g, (c) => `\\${c}`);

function pageContent(spec: SyntheticSpec, index: number): string {
  const lines = [
    "BT /F2 16 Tf 40 540 Td", `(${escape(spec.title)}) Tj`, "ET",
    "BT /F1 11 Tf 40 500 Td 15 TL",
    `(Physical page ${index + 1} of ${spec.printed.length}.) Tj T*`,
    ...BODY.map((line) => `(${escape(line)}) Tj T*`),
    "ET",
  ];
  const printed = spec.printed[index];
  if (printed !== null && printed !== undefined) lines.push("BT /F1 10 Tf 200 30 Td", `(${escape(printed)}) Tj`, "ET");
  return lines.join("\n");
}

function labelDict(range: LabelRange): string {
  const parts = ["/Type /PageLabel"];
  if (range.style) parts.push(`/S /${range.style}`);
  if (range.prefix !== undefined) parts.push(`/P (${escape(range.prefix)})`);
  if (range.first !== undefined) parts.push(`/St ${range.first}`);
  return `${range.start} << ${parts.join(" ")} >>`;
}

/** One small, valid PDF 1.7 file: base-14 Times fonts, one content stream per page. */
export function syntheticPdf(spec: SyntheticSpec): Uint8Array {
  const count = spec.printed.length;
  const objects: string[] = [];
  const firstPage = 5;
  const kids = Array.from({ length: count }, (_, i) => `${firstPage + i * 2} 0 R`).join(" ");
  const pageLabels = spec.labels ? ` /PageLabels << /Nums [${spec.labels.map(labelDict).join(" ")}] >>` : "";
  objects[1] = `<< /Type /Catalog /Pages 2 0 R${pageLabels} >>`;
  objects[2] = `<< /Type /Pages /Kids [${kids}] /Count ${count} >>`;
  objects[3] = "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman /Encoding /WinAnsiEncoding >>";
  objects[4] = "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Bold /Encoding /WinAnsiEncoding >>";
  for (let i = 0; i < count; i++) {
    const page = firstPage + i * 2;
    const content = pageContent(spec, i);
    objects[page] = `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 420 595] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents ${page + 1} 0 R >>`;
    objects[page + 1] = `<< /Length ${content.length} >>\nstream\n${content}\nendstream`;
  }
  const info = objects.length;
  objects[info] = `<< /Title (${escape(spec.title)}) /Producer (Ibis synthetic fixture) >>`;
  let out = "%PDF-1.7\n";
  const offsets: number[] = [];
  for (let n = 1; n < objects.length; n++) {
    offsets[n] = out.length;
    out += `${n} 0 obj\n${objects[n]}\nendobj\n`;
  }
  const xref = out.length;
  out += `xref\n0 ${objects.length}\n0000000000 65535 f \n`;
  for (let n = 1; n < objects.length; n++) out += `${String(offsets[n]).padStart(10, "0")} 00000 n \n`;
  out += `trailer\n<< /Size ${objects.length} /Root 1 0 R /Info ${info} 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  // ASCII only, so string length is byte length and the offsets hold.
  return new TextEncoder().encode(out);
}

/** Bytes that start like a PDF and are nothing of the kind. */
export function brokenPdf(): Uint8Array {
  return new TextEncoder().encode("%PDF-1.7\nThis invented file stops before any object.\n");
}
