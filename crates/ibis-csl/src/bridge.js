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
    const outputs = {};
    for (const format of ["text", "html", "rtf"]) {
      engine.setOutputFormat(format);
      // Clear retained records as well as IDs: a later snapshot may reuse an ID.
      engine.updateItems([]);
      engine.updateItems(request.items.map((item) => item.id));
      let output;
      if (request.bibliography) {
        const bibliography = engine.makeBibliography();
        if (!bibliography || bibliography[0].bibliography_errors.length) throw Error("style could not format the bibliography");
        if (format === "text") outputs.text_parts = bibliography[1];
        output = bibliography[1].map(oneLine).join("\n");
      } else {
        output = engine.makeCitationCluster(request.cites);
        if (engine.tmp.citation_errors.length) throw Error("style could not format the citation");
      }
      outputs[format] = format === "rtf" ? wrapRtf(output, request.bibliography) : output;
    }
    return JSON.stringify(outputs);
  };
})();
