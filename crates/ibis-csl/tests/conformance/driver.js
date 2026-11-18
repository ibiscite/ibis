// Runs one CSL test-suite fixture the way citeproc-js's own runner does
// (adapted from citeproc-test-runner's lib/sys.js), for the modes and sections the
// suite's processor tests use. The same file runs under Node with the plain
// processor and inside Ibis's QuickJS runtime, so only the runtime differs.
//
// `fixture` is { csl, mode, input, citationItems?, citations? }; `items`, when
// given, replaces `input` as what the processor reads (Ibis's own mapping).
//
// Copyright (c) Frank Bennett
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.
(() => {
  "use strict";
  // The runner teaches every engine Turkish month names; a few fixtures use them.
  const months = ["ocak", "Şubat", "mart", "nisan", "mayıs", "haziran", "temmuz", "ağustos", "eylül", "ekim", "kasım", "aralık", "bahar", "yaz", "sonbahar", "kış"];

  // processCitationCluster's results, kept in document order: ">>" marks a
  // citation the last call changed, ".." one it left alone.
  const updateDoc = (engine, citations, doc) => {
    for (const citation of citations) {
      const [, result] = engine.processCitationCluster(citation[0], citation[1], citation[2]);
      for (let j = doc.length - 1; j > -1; j--) {
        if (!engine.registry.citationreg.citationById[doc[j].citationID]) doc.splice(j, 1);
      }
      const position = new Map(citation[1].concat(citation[2]).map(([id], index) => [id, index]));
      doc.sort((a, b) => (position.get(a.citationID) ?? NaN) > (position.get(b.citationID) ?? NaN) ? 1
        : (position.get(a.citationID) ?? NaN) < (position.get(b.citationID) ?? NaN) ? -1 : 0);
      for (const entry of doc) entry.prefix = "..";
      const pending = result.map((insert) => {
        const existing = doc.findIndex((entry) => entry.citationID === insert[2]);
        if (existing < 0) return insert;
        doc[existing] = { prefix: ">>", citationID: insert[2], text: insert[1] };
        return null;
      });
      for (const insert of pending) {
        if (insert) doc.splice(insert[0], 0, { prefix: ">>", citationID: insert[2], text: insert[1] });
      }
    }
  };

  globalThis.__cslFixture = (fixture, items) => {
    const records = {};
    const ids = [];
    for (const item of items ?? fixture.input) {
      records[item.id] = item;
      ids.push(item.id);
    }
    const sys = {
      retrieveItem: (id) => records[id],
      retrieveLocale: (tag) => __ibisLocaleXml(tag) ?? false,
    };
    const engine = new CSL.Engine(sys, fixture.csl);
    engine.fun.dateparser.addDateParserMonths(months);
    engine.setLangPrefsForCites({
      persons: ["translit"], institutions: ["translit"], titles: ["translit", "translat"],
      journals: ["translit"], publishers: ["translat"], places: ["translat"],
    });
    let citationItems = fixture.citationItems;
    if (!fixture.citations) engine.updateItems(ids);
    if (!citationItems && !fixture.citations) {
      citationItems = [engine.registry.reflist.map((entry) => ({ id: entry.id }))];
    }
    let output;
    if (citationItems) {
      output = citationItems.map((citation) => engine.makeCitationCluster(citation)).join("\n");
    } else {
      const doc = [];
      updateDoc(engine, fixture.citations, doc);
      output = doc.map((entry, index) => `${entry.prefix}[${index}] ${entry.text}`).join("\n");
    }
    if (fixture.mode === "bibliography") {
      const [meta, entries] = engine.makeBibliography();
      output = meta.bibstart + entries.join("") + meta.bibend;
    } else if (fixture.mode !== "citation") {
      throw Error(`unknown mode ${fixture.mode}`);
    }
    return output.replace(/\r\n?/g, "\n");
  };
})();
