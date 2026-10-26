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
      if (typeof node === "string") printed ||= node.trim().length > 0;
      else if (node && typeof node.blobs === "string") printed ||= node.blobs.trim().length > 0;
      else if (node && Array.isArray(node.blobs)) for (const child of node.blobs) pending.push(child);
    }
    if (!printed) throw unverified;
    const id = String(args[0].id);
    capture.seen.set(id, (capture.seen.get(id) ?? 0) + 1);
    return result;
  };
  const oneLine = (text) => text.replace(/\s*\n\s*/g, " ").trim();
  const wrapRtf = (text, bibliography) => `{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0 Times New Roman;}}\\f0\n${bibliography ? text.split("\n").join("\\par\n") : text}\n}`;
  globalThis.__ibisFormat = (request) => {
    let xml = styles.get(request.style).replace(/disambiguate-add-year-suffix="true"/g, 'disambiguate-add-year-suffix="false"');
    // A selected bibliography cannot know a document's citation numbers.
    if (request.style === "ieee") xml = xml.replace('<text variable="citation-number" prefix="[" suffix="]"/>', "").replace(' second-field-align="flush"', "");
    activeItems = new Map(request.items.map((item) => [item.id, item]));
    const key = JSON.stringify([request.style, request.locale, request.keep_order]);
    let engine = engines.get(key);
    if (!engine) {
      engine = new CSL.Engine({ retrieveLocale: () => __ibisLocale, retrieveItem: (id) => activeItems.get(id) }, xml, request.locale ?? undefined, request.locale !== null);
      engine.opt.development_extensions.throw_on_empty = true;
      if (request.keep_order) engine.citation_sort.tokens = [];
      if (engines.size >= 8) engines.delete(engines.keys().next().value);
      engines.set(key, engine);
    }
    const outputs = { status: "rendered" };
    const expected = new Map();
    for (const cite of request.cites) expected.set(cite.id, request.bibliography ? 1 : (expected.get(cite.id) ?? 0) + 1);
    try {
      for (const format of ["text", "html", "rtf"]) {
        engine.setOutputFormat(format);
        // Clear retained records as well as IDs: a later snapshot may reuse an ID.
        engine.updateItems([]);
        engine.updateItems(request.items.map((item) => item.id));
        const capture = { engine, area: request.bibliography ? "bibliography" : "citation", seen: new Map() };
        observation = capture;
        let output;
        if (request.bibliography) {
          const bibliography = engine.makeBibliography();
          if (!bibliography || bibliography[0].bibliography_errors.length) throw unverified;
          if (format === "text") outputs.text_parts = bibliography[1];
          output = bibliography[1].map(oneLine).join("\n");
        } else {
          output = engine.makeCitationCluster(request.cites);
          if (engine.tmp.citation_errors.length) throw unverified;
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
    }
    outputs.coverage = request.cites.map((_, index) => index);
    return JSON.stringify(outputs);
  };
})();
