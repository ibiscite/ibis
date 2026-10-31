// Trusted adapter shared by QuickJS and the Node differential oracle.
(() => {
  "use strict";
  CSL.debug = () => {};
  CSL.error = (message) => {
    // The pinned processor catches native errors while building trusted style
    // attributes (and previews). Preserve only those exact forwarding shapes.
    const cause = typeof message === "string" && /^(?:@[a-z-]+ attribute: |Error running CSL processor for preview: )(RangeError: Maximum call stack size exceeded|InternalError: out of memory(?: in regexp execution)?)$/.exec(message)?.[1];
    if (cause === "RangeError: Maximum call stack size exceeded") throw RangeError("Maximum call stack size exceeded");
    if (cause) {
      const error = Error("out of memory");
      error.name = "InternalError";
      throw error;
    }
    throw Error("citation formatting failed");
  };
  const styles = new Map(__ibisStyles);
  const engines = new Map();
  let activeItems = new Map();
  // The source adapters' canonical rich fields escape text separately from
  // markup. citeproc expects decoded text, but must never parse an escaped
  // literal `<i>` as an instruction. Decode once before casing/sorting and
  // restore only literal less-than signs at the final text-escaping boundary.
  const richFields = ["title", "title-short", "container-title", "container-title-short", "volume-title", "volume-title-short"];
  const entities = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: "\u00a0" };
  const bidi = /[\u061c\u200e\u200f\u202a-\u202e\u2066-\u206f]/g;
  const decodeText = (text, lessThan) => text.replace(/&(?:amp|lt|gt|quot|apos|nbsp|#[0-9]{1,7}|#x[0-9a-fA-F]{1,6});/g, (entity) => {
    const name = entity.slice(1, -1);
    let decoded = entities[name];
    if (decoded === undefined) {
      const code = name.startsWith("#x") ? parseInt(name.slice(2), 16) : Number(name.slice(1));
      if (code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff)) return entity;
      decoded = String.fromCodePoint(code);
    }
    return decoded.replace(bidi, "");
  }).replace(/</g, () => lessThan);
  // Match the pinned processor's existing tag spellings/normalization before
  // decoding text. An entity in an unrecognized tag or attribute cannot create
  // a new formatting instruction. Keep native sc/nodecor support for CSL-JSON.
  const markup = /(<\/?(?:i|b|sc|sub|sup)>|<span\s+class="no(?:case|decor)"[^>]*>|<span\s+style="font-variant:\s*small-caps;?"[^>]*>|<\/span>)/g;
  const decodeRich = (text, lessThan = "<") => text.split(markup)
    .map((part, index) => index % 2 ? part : decodeText(part, lessThan)).join("");
  let literalLessThan = null;
  for (const format of ["text", "html", "rtf"]) {
    const escape = CSL.Output.Formats[format].text_escape;
    CSL.Output.Formats[format].text_escape = (text) => escape(literalLessThan && text ? text.split(literalLessThan).join("<") : text);
  }
  const processorItems = (request) => {
    // Include decoded data in the collision check: numeric entities can spell
    // the sentinel too. This non-overlapping sentinel cannot consume a suffix
    // of adjacent source text. Doubling bounds the total collision-scan work.
    const decoded = request.items.map((item) => Object.fromEntries(richFields
      .filter((field) => typeof item[field] === "string")
      .map((field) => [field, decodeRich(item[field])])));
    const source = JSON.stringify([request, decoded]);
    let prefix = "\uE002";
    while (source.includes(prefix + "\uE003")) prefix += prefix;
    literalLessThan = prefix + "\uE003";
    return request.items.map((item) => ({ ...item, ...Object.fromEntries(richFields
      .filter((field) => typeof item[field] === "string")
      .map((field) => [field, decodeRich(item[field], literalLessThan)])) }));
  };
  // Observe only each final cite's newly emitted tree. Do not enable citation
  // wrappers, render again, alter blobs, or infer errors from output strings.
  const unverified = Object.freeze({});
  let observation = null;
  // An affix alone does not make a cite printed.
  const visible = (text) => text.replace(/[;.,]?\uE000[\uE010-\uE019]+\uE001[;.,]?/g, "").trim().length > 0;
  const getCite = CSL.getCite;
  CSL.getCite = function (...args) {
    const capture = observation;
    if (!capture || capture.engine !== this || this.tmp.just_looking || this.tmp.area !== capture.area) return getCite.apply(this, args);
    const parent = this.output.current.value();
    const children = Array.isArray(parent) ? parent : parent?.blobs;
    if (!Array.isArray(children)) throw unverified;
    const offset = children.length;
    const result = getCite.apply(this, args);
    if (this.output.current.value() !== parent) throw unverified;
    const pending = children.slice(offset);
    let printed = false;
    const yearOnly = capture.format === "text" && args[1]?.["year-only"];
    const issued = args[0].issued;
    let formattedYear = false;
    let noDate = false;
    while (pending.length) {
      const node = pending.pop();
      const value = typeof node === "string" ? node : typeof node?.blobs === "string" ? node.blobs : null;
      if (value !== null) {
        printed ||= visible(value);
        if (yearOnly) {
          // CSL.Blob preserves the term token's identity after localization and
          // text formatting. Unrelated source text cannot supply this metadata.
          noDate ||= node.strings?.term === "no date" && visible(value);
          // Date-part blobs retain the style's year form (for example, 26).
          if (node.strings?.name === "year") {
            const format = CSL.Util.Dates.year[node.strings.form ?? "long"];
            const expected = issued?.literal ?? (issued?.year !== undefined && typeof format === "function" && format(this, Math.abs(Number(issued.year))));
            formattedYear ||= Boolean(expected) && (issued?.literal ? value.includes(expected) : value.split(/\D+/).includes(String(expected)));
          }
        }
      } else if (node && Array.isArray(node.blobs)) for (const child of node.blobs) pending.push(child);
    }
    if (!printed) throw unverified;
    if (yearOnly) {
      // citeproc uses zero as its undated year sentinel.
      if (issued?.literal || (issued?.year !== undefined && Number(issued.year) !== 0)) {
        if (!formattedYear) throw unverified;
      } else if (!noDate) throw unverified;
    }
    const id = String(args[0].id);
    capture.seen.set(id, (capture.seen.get(id) ?? 0) + 1);
    return result;
  };
  // Affixes reach citeproc as inert placeholders; each is written here as the
  // person typed it, escaped and styled by citeproc's own output format, with
  // the spacing citeproc would add. A placeholder that is missing or repeated
  // means the processor moved or dropped an affix, so nothing is shown.
  const styleTemplates = [["italic", "@font-style/italic"], ["bold", "@font-weight/bold"]];
  const scriptTemplates = { superscript: "@vertical-align/sup", subscript: "@vertical-align/sub" };
  const writeAffixes = (engine, format, output, affixes) => {
    const formatter = CSL.Output.Formats[format];
    for (const affix of affixes) {
      const plain = affix.runs.map((run) => run.text).join("");
      let written = affix.runs.map((run) => {
        let text = formatter.text_escape(run.text.replace(/\s+/g, " "));
        const templates = styleTemplates.filter(([flag]) => run[flag]).map(([, name]) => name);
        if (scriptTemplates[run.script]) templates.push(scriptTemplates[run.script]);
        for (const name of templates) {
          if (typeof formatter[name] === "string") text = formatter[name].replace("%%STRING%%", () => text);
        }
        return text;
      }).join("");
      if (affix.kind === "prefix" && CSL.checkPrefixSpaceAppend(engine, plain).endsWith(" ") && !/\s$/.test(plain)) written += " ";
      if (affix.kind === "suffix" && CSL.checkSuffixSpacePrepend(engine, plain) !== plain) written = " " + written;
      const parts = output.split(formatter.text_escape(affix.placeholder));
      if (parts.length !== 2) throw unverified;
      output = parts.join(written);
    }
    return output;
  };
  // A narrative citation is the author as the sentence's subject, then the
  // rest of the cite with the author suppressed: Taylor (2026, p. 12). The
  // prefix goes before the name; the locator and suffix stay in the cite.
  const cluster = (engine, cites) => {
    const make = (list) => {
      const output = engine.makeCitationCluster(list);
      if (engine.tmp.citation_errors.length) throw unverified;
      return output;
    };
    if (!(cites.length === 1 && cites[0].narrative)) return make(cites);
    const { narrative, prefix, suffix, locator, label, ...cite } = cites[0];
    const author = make([{ ...cite, prefix, "author-only": true, "suppress-author": false }]);
    const rest = make([{ ...cite, suffix, locator, label, "suppress-author": true, "author-only": false }]);
    return `${author} ${rest}`;
  };
  const oneLine = (text) => text.replace(/\s*\n\s*/g, " ").trim();
  const wrapRtf = (text, bibliography) => `{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0 Times New Roman;}}\\f0\n${bibliography ? text.split("\n").join("\\par\n") : text}\n}`;
  globalThis.__ibisFormat = (request) => {
    let xml = styles.get(request.style).replace(/disambiguate-add-year-suffix="true"/g, 'disambiguate-add-year-suffix="false"');
    activeItems = new Map(processorItems(request).map((item) => [item.id, item]));
    const key = JSON.stringify([request.style, request.locale, request.keep_order]);
    let engine = engines.get(key);
    if (!engine) {
      // Keep one warm style, rather than eight independent item registries.
      engines.clear();
      if (request.numeric) {
        const tree = CSL.parseXml(xml);
        const macros = new Map(tree.children.filter((node) => node.name === "macro").map((node) => [node.attrs.name, node]));
        const copies = new Map();
        let serial = 0;
        // A selected bibliography cannot know a document's citation numbers.
        // Use the processor's XML tree so quoting and affixes do not matter.
        for (const bibliography of tree.children.filter((node) => node.name === "bibliography")) {
          delete bibliography.attrs["second-field-align"];
          const pending = bibliography.children.filter((node) => node.name === "layout");
          while (pending.length) {
            const node = pending.pop();
            node.children = node.children.filter((child) => !(child.name === "text" && child.attrs.variable === "citation-number"));
            for (const child of node.children) {
              const called = child.attrs?.macro;
              if (macros.has(called)) {
                if (!copies.has(called)) {
                  // Shared citation and sort macros keep their original numbers.
                  const copy = JSON.parse(JSON.stringify(macros.get(called)));
                  let name;
                  do { name = `ibis-bibliography-${serial++}`; } while (macros.has(name));
                  copy.attrs.name = name;
                  macros.set(name, copy);
                  copies.set(called, name);
                  tree.children.push(copy);
                  pending.push(copy);
                }
                child.attrs.macro = copies.get(called);
              }
              if (child.children) pending.push(child);
            }
          }
        }
        xml = tree;
      }
      engine = new CSL.Engine({ retrieveLocale: () => __ibisLocale, retrieveItem: (id) => activeItems.get(id) }, xml, request.locale ?? undefined, request.locale !== null);
      engine.opt.development_extensions.throw_on_empty = true;
      if (request.keep_order) engine.citation_sort.tokens = [];
      engines.set(key, engine);
    }
    const outputs = { status: "rendered" };
    const expected = new Map();
    for (const cite of request.cites) expected.set(cite.id, request.bibliography ? 1 : (expected.get(cite.id) ?? 0) + (cite.narrative ? 2 : 1));
    try {
      // Register and disambiguate the selected records once. The three output
      // modes serialize that same processor state, rather than rebuilding it.
      engine.setOutputFormat("text");
      engine.updateItems(request.items.map((item) => item.id));
      for (const format of ["text", "html", "rtf"]) {
        engine.setOutputFormat(format);
        const capture = { engine, format, area: request.bibliography ? "bibliography" : "citation", seen: new Map() };
        observation = capture;
        let output;
        if (request.bibliography) {
          const bibliography = engine.makeBibliography();
          if (!bibliography || bibliography[0].bibliography_errors.length) throw unverified;
          if (format === "text") outputs.text_parts = bibliography[1];
          output = bibliography[1].map(oneLine).join("\n");
        } else {
          output = cluster(engine, request.cites);
          output = writeAffixes(engine, format, output, request.affixes ?? []);
        }
        observation = null;
        if (capture.seen.size !== expected.size || [...expected].some(([id, count]) => capture.seen.get(id) !== count)) throw unverified;
        outputs[format] = format === "rtf" ? wrapRtf(output, request.bibliography) : output;
      }
    } catch (error) {
      if (error !== unverified) throw error;
      // A partially evaluated engine must never be reused after a refusal.
      engines.delete(key);
      return JSON.stringify({ status: "refused" });
    } finally {
      observation = null;
      // No cached engine keeps the last request's source records alive.
      activeItems.clear();
      engine.updateItems([]);
      literalLessThan = null;
    }
    outputs.coverage = request.cites.map((_, index) => index);
    return JSON.stringify(outputs);
  };
})();
