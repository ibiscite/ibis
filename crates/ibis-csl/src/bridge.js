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
    while (pending.length) {
      const node = pending.pop();
      if (typeof node === "string") printed ||= visible(node);
      else if (node && typeof node.blobs === "string") printed ||= visible(node.blobs);
      else if (node && Array.isArray(node.blobs)) for (const child of node.blobs) pending.push(child);
    }
    if (!printed) throw unverified;
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
    activeItems = new Map(request.items.map((item) => [item.id, item]));
    const key = JSON.stringify([request.style, request.locale, request.keep_order]);
    let engine = engines.get(key);
    if (!engine) {
      // Keep one warm style, rather than eight independent item registries.
      engines.clear();
      if (request.numeric) {
        const tree = CSL.parseXml(xml);
        // A selected bibliography cannot know a document's citation numbers.
        // Use the processor's XML tree so quoting and affixes do not matter.
        for (const bibliography of tree.children.filter((node) => node.name === "bibliography")) {
          delete bibliography.attrs["second-field-align"];
          const pending = bibliography.children.filter((node) => node.name === "layout");
          while (pending.length) {
            const node = pending.pop();
            node.children = node.children.filter((child) => !(child.name === "text" && child.attrs.variable === "citation-number"));
            for (const child of node.children) if (child.children) pending.push(child);
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
        const capture = { engine, area: request.bibliography ? "bibliography" : "citation", seen: new Map() };
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
    }
    outputs.coverage = request.cites.map((_, index) => index);
    return JSON.stringify(outputs);
  };
})();
