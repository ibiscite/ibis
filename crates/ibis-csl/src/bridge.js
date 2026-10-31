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
  const dateAsSortKey = CSL.dateAsSortKey;
  CSL.dateAsSortKey = function (state, item, isMacro) {
    const variable = this.variables[0];
    const sort = activeItems.get(String(item.id))?.["ibis-date-sort"]?.[variable];
    if (!sort) return dateAsSortKey.call(this, state, item, isMacro);
    // Only the sort call sees a numeric year. The registered date remains a
    // literal, so rendering never duplicates its year or drops its letters.
    const date = { ...item[variable], year: sort.year };
    delete date.raw;
    delete date["date-parts"];
    dateAsSortKey.call(this, state, { ...item, [variable]: date }, isMacro);
    // Source letters follow the processor's a..z, aa.. sequence. Length-first
    // ordering avoids both lexicographic aa-before-b and numeric overflow.
    // Numeric collation ignores punctuation: an alphabetic boundary prevents
    // the length digits from enlarging the processor's numeric date key.
    const suffix = sort.suffix ? "s" + String(sort.suffix.length).padStart(8, "0") + sort.suffix : "";
    state.output.append(suffix, isMacro && state.tmp.extension ? "macro-with-date" : "empty");
  };
  // The source adapters' canonical rich fields escape text separately from
  // markup. citeproc expects decoded text, but must never parse an escaped
  // literal `<i>` as an instruction. Decode once before casing/sorting and
  // restore only literal less-than signs at the final text-escaping boundary.
  const richFields = ["title", "title-short", "container-title", "container-title-short", "volume-title", "volume-title-short"];
  const entities = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: "\u00a0" };
  // Direction marks carry ordinary RTL/LTR meaning. Remove only embedding,
  // override and isolate controls that can escape the field's text.
  const bidi = /[\u202a-\u202e\u2066-\u206f]/g;
  // Entity decoding creates new characters after Rust's inline normalization.
  // Apply the same line/control rules before citeproc sees those characters.
  const inline = (text) => text
    .replace(/^[\r\n\u0085\u2028\u2029]+|[\r\n\u0085\u2028\u2029]+$/g, "")
    .replace(/[\r\n\u0085\u2028\u2029]+/g, " ")
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u0084\u0086-\u009f]/g, " ")
    .replace(bidi, "");
  const decodeText = (text, lessThan) => text.replace(/&(?:amp|lt|gt|quot|apos|nbsp|#[0-9]{1,7}|#x[0-9a-fA-F]{1,6});/g, (entity) => {
    const name = entity.slice(1, -1);
    let decoded = entities[name];
    if (decoded === undefined) {
      const code = name.startsWith("#x") ? parseInt(name.slice(2), 16) : Number(name.slice(1));
      if (code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff)) return entity;
      decoded = String.fromCodePoint(code);
    }
    return decoded;
  }).replace(/</g, () => lessThan);
  // Match the pinned processor's existing tag spellings/normalization before
  // decoding text. An entity in an unrecognized tag or attribute cannot create
  // a new formatting instruction. Keep native sc/nodecor support for CSL-JSON.
  const markup = /(<\/?(?:i|b|sc|sub|sup)>|<span\s+class="no(?:case|decor)"[^>]*>|<span\s+style="font-variant:\s*small-caps;?"[^>]*>|<\/span>)/g;
  const decodeRich = (text, lessThan = "<") => inline(text.split(markup)
    .map((part, index) => index % 2 ? part : decodeText(part, lessThan)).join(""));
  let literalLessThan = null;
  for (const format of ["text", "html", "rtf"]) {
    const escape = CSL.Output.Formats[format].text_escape;
    CSL.Output.Formats[format].text_escape = (text) => escape(literalLessThan && text ? text.split(literalLessThan).join("<") : text);
  }
  const processorItems = (request) => {
    // A single unused private-use code point cannot be assembled by removing
    // markup between source characters, and casing cannot change it. Include
    // decoded data too: numeric entities can spell an existing private symbol.
    const decoded = request.items.map((item) => Object.fromEntries(richFields
      .filter((field) => typeof item[field] === "string")
      .map((field) => [field, decodeRich(item[field])])));
    const used = new Set(JSON.stringify([request, decoded]));
    literalLessThan = null;
    for (const [start, end] of [[0xe100, 0xf8ff], [0xf0000, 0xffffd], [0x100000, 0x10fffd]]) {
      for (let code = start; code <= end; code++) {
        const candidate = String.fromCodePoint(code);
        if (!used.has(candidate)) { literalLessThan = candidate; break; }
      }
      if (literalLessThan) break;
    }
    if (!literalLessThan) return null;
    return request.items.map((item) => ({ ...item, ...Object.fromEntries(richFields
      .filter((field) => typeof item[field] === "string")
      .map((field) => [field, decodeRich(item[field], literalLessThan)])) }));
  };
  // Observe each final cite's newly emitted tree without enabling wrappers,
  // rendering it again or changing blobs. Ownership follows those exact roots
  // through the processor's normal serialization, including sorting/collapse.
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
    const identity = String(args[0].id);
    const occurrence = capture.occurrences.get(identity) ?? 0;
    capture.occurrences.set(identity, occurrence + 1);
    const indices = capture.indices.get(identity);
    const owner = indices?.[Math.min(occurrence, indices.length - 1)];
    for (const root of pending) if (root && typeof root === "object") capture.roots.set(root, owner);
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
  const serialize = CSL.Output.Queue.prototype.string;
  CSL.Output.Queue.prototype.string = function (state, blobs, parent) {
    const capture = observation;
    const owner = capture?.format === "text" && blobs.length === 1 ? capture.roots.get(blobs[0]) : undefined;
    const result = serialize.call(this, state, blobs, parent);
    if (owner !== undefined) {
      const parts = typeof result === "string" ? [result] : result;
      if (!Array.isArray(parts)) throw unverified;
      capture.parts.push({ item: owner, parts: [...parts] });
    }
    return result;
  };
  // These are the processor's final serialized chunks, not searches for source
  // names/years. Shared layout punctuation remains unowned. Full reconstruction
  // and one appearance per cite are validated again at the Rust boundary.
  const spans = (text, parts) => {
    const nodes = [];
    let offset = 0;
    for (const part of parts) {
      const start = text.indexOf(part.text, offset);
      if (start < 0) throw unverified;
      if (start > offset) nodes.push({ item: null, text: text.slice(offset, start) });
      nodes.push({ item: part.item, text: part.text });
      offset = start + part.text.length;
    }
    if (offset < text.length) nodes.push({ item: null, text: text.slice(offset) });
    return nodes;
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
    const converted = processorItems(request);
    if (!converted) return JSON.stringify({ status: "refused" });
    activeItems = new Map(converted.map((item) => [item.id, item]));
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
        const indices = new Map();
        request.cites.forEach((cite, index) => indices.set(cite.id, [...indices.get(cite.id) ?? [], index]));
        const capture = { engine, format, area: request.bibliography ? "bibliography" : "citation", seen: new Map(), roots: new WeakMap(), parts: [], indices, occurrences: new Map() };
        observation = capture;
        let output;
        if (request.bibliography) {
          const bibliography = engine.makeBibliography();
          if (!bibliography || bibliography[0].bibliography_errors.length) throw unverified;
          if (format === "text") outputs.text_parts = bibliography[1];
          output = bibliography[1].map(oneLine).join("\n");
          if (format === "text") {
            outputs.spans = [];
            bibliography[1].forEach((part, index) => {
              const ids = bibliography[0].entry_ids[index];
              if (ids.length !== 1) throw unverified;
              if (index) outputs.spans.push({ item: null, text: "\n" });
              const occurrences = indices.get(String(ids[0]));
              if (!occurrences?.length) throw unverified;
              occurrences.forEach((item, occurrence) => outputs.spans.push({ item, text: occurrence ? "" : oneLine(part) }));
            });
          }
        } else {
          output = cluster(engine, request.cites);
          output = writeAffixes(engine, format, output, request.affixes ?? []);
          if (format === "text") {
            const parts = capture.parts.map((part) => {
              // Numeric locators remain NumberBlobs until the processor's final
              // range pass. Serialize copies of those now-final leaves only;
              // no cite tree is evaluated again, and no processor blob changes.
              const text = part.parts.map((leaf) => typeof leaf === "string" ? leaf : engine.output.renderBlobs([
                { ...leaf, strings: { ...leaf.strings }, decorations: [...leaf.decorations], checkNext: null, checkLast: null },
              ])).join("");
              return { item: part.item, text: writeAffixes(engine, format, text, (request.affixes ?? []).filter((affix) => text.includes(affix.placeholder))) };
            });
            outputs.spans = spans(output, parts);
          }
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
