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
  // Core supplies checked, decoded text runs separately from literal markup.
  const markup = /(<\/?(?:i|b|sc|sub|sup)>|<span\s+class="no(?:case|decor)"[^>]*>|<span\s+style="font-variant:\s*small-caps;?"[^>]*>|<\/span>)/g;
  // Periods cannot distinguish a subtitle from an abbreviation ("vs. y").
  // Keep the processor's explicit subtitle delimiters and capitalization,
  // but never split a protected span or a markup attribute into case fragments.
  const subtitleStops = ["!\\s+", "\\?\\s+", "\\s*:+\\s+", "\\s*—\\s*", "\\s+-\\s+", "\\s*---*\\s*"];
  const subtitlePattern = {
    match: new RegExp("(" + subtitleStops.join("|") + ")", "g"),
    matchfirst: new RegExp("^(" + subtitleStops.join("|") + ")"),
    split: new RegExp("(?:" + subtitleStops.join("|") + ")"),
  };
  const splitSubtitle = (text) => {
    if (!text) return text;
    const parts = [], spans = [];
    let cursor = 0, start = 0, protectedDepth = 0;
    const stopsThrough = (end) => {
      if (protectedDepth) return;
      for (const stop of text.slice(cursor, end).matchAll(subtitlePattern.match)) {
        const offset = cursor + stop.index;
        parts.push(text.slice(start, offset).trim(), stop[0]);
        start = offset + stop[0].length;
      }
    };
    for (const tag of text.matchAll(markup)) {
      stopsThrough(tag.index);
      if (tag[0] === "</span>") {
        if (spans.pop()) protectedDepth--;
      } else if (tag[0].startsWith("<span")) {
        const protectedSpan = /^<span\s+class="nocase"/.test(tag[0]);
        spans.push(protectedSpan);
        if (protectedSpan) protectedDepth++;
      }
      cursor = tag.index + tag[0].length;
    }
    stopsThrough(text.length);
    parts.push(text.slice(start));
    return parts;
  };
  const withSubtitleBoundaries = (state, render, receiver, args) => {
    if (!state.opt.development_extensions.uppercase_subtitles) return render.apply(receiver, args);
    const pattern = CSL.TITLE_SPLIT_REGEXP, split = CSL.TITLE_SPLIT;
    CSL.TITLE_SPLIT_REGEXP = subtitlePattern;
    CSL.TITLE_SPLIT = splitSubtitle;
    try { return render.apply(receiver, args); }
    finally { CSL.TITLE_SPLIT_REGEXP = pattern; CSL.TITLE_SPLIT = split; }
  };
  const extractTitle = CSL.extractTitleAndSubtitle, caseTitle = CSL.titlecaseSentenceOrNormal;
  CSL.extractTitleAndSubtitle = function (...args) { return withSubtitleBoundaries(this, extractTitle, this, args); };
  CSL.titlecaseSentenceOrNormal = function (...args) { return withSubtitleBoundaries(args[0], caseTitle, this, args); };
  // The processor's text format opens a block with a line break but never
  // closes it, gluing the next word to the block's last one: "Noor2021". HTML
  // shows the break, so the text keeps it too, and one-line folding spaces it.
  CSL.Output.Formats.text["@display/block"] = (state, str) => "\n" + str + "\n";
  CSL.Output.Formats.text["@display/indent"] = (state, str) => "\n    " + str + "\n";
  // The processor opens a style's date output level with the date's own node
  // at build time, affixes included; only after its first range does it reopen
  // with the affix-free copy. So the first year range a compiled style prints
  // gets the affixes twice: "(2020–(2026))". Open with the copy from the start.
  // A range's second date reopens with the copy of whichever date the style
  // built last, so a citation borrowed the bibliography's bold: "2021–**2024**".
  // The date's own level already formats the whole range, so the copy left for
  // that reopening formats nothing.
  const buildDate = CSL.Node.date.build;
  CSL.Node.date.build = function (state, target) {
    if (this.tokentype !== CSL.START && this.tokentype !== CSL.SINGLETON) return buildDate.call(this, state, target);
    const node = this;
    state.dateput.openLevel = function (token) {
      return CSL.Output.Queue.prototype.openLevel.call(this, token === node ? state.tmp.date_token : token);
    };
    try { return buildDate.call(this, state, target); } finally {
      delete state.dateput.openLevel;
      state.tmp.date_token = CSL.Util.cloneToken(state.tmp.date_token);
      state.tmp.date_token.decorations = [];
    }
  };
  let literalLessThan = null;
  const checkedText = (text) => literalLessThan && text ? text.split(literalLessThan).join("<") : text;
  // A DOI is encoded for a URL exactly once, as the record is read
  // (`doiForUrl` below): the processor encodes one DOI up to three times on
  // its way to a link, so its own encoding does nothing.
  CSL.Util.encodeDoiForUrl = (doi) => doi;
  // The processor links every URL and DOI by prefixing doi.org to anything
  // without a lower-case scheme, and builds the link from the escaped text.
  // Here a web address or a DOI links, with the address taken back from the
  // text; text that cannot be one address, such as one with a space or a
  // tag in it, stays unlinked.
  const link = (state, text) => {
    const shown = text.replace(/&#(38|60|62);/g, (_, code) => String.fromCharCode(Number(code)));
    const target = /^https?:\/\//i.test(shown) ? shown : /^10\./.test(shown) ? `https://doi.org/${shown}` : null;
    if (!target || /[\s"<>\\]/.test(target)) return text;
    return `<a href="${target.replace(/&/g, "&#38;")}">${text}</a>`;
  };
  CSL.Output.Formats.html["@DOI/true"] = link;
  CSL.Output.Formats.html["@URL/true"] = link;
  for (const format of ["text", "html", "rtf"]) {
    const escape = CSL.Output.Formats[format].text_escape;
    CSL.Output.Formats[format].text_escape = (text) => escape(checkedText(text));
  }
  // A record's DOI, bare and encoded once for a URL. A doi.org address is
  // read back to its DOI first; another address is left as it is.
  const doiForUrl = (doi) => {
    const resolver = /^https?:\/\/(dx\.)?doi\.org\//i.exec(doi);
    if (!resolver && /^https?:\/\//i.test(doi)) return doi;
    let bare = resolver ? doi.slice(resolver[0].length) : doi;
    if (resolver) try { bare = decodeURIComponent(bare); } catch { /* kept as written */ }
    return bare.replace(/[\u0000- "#%<>?[\\\]^`{|}\u007F-\u009F]/g, encodeURIComponent);
  };
  const processorItems = (request) => {
    // One unused symbol protects literal less-than signs in every text scalar.
    const used = new Set(JSON.stringify(request));
    literalLessThan = null;
    for (const [start, end] of [[0xe100, 0xf8ff], [0xf0000, 0xffffd], [0x100000, 0x10fffd]]) {
      for (let code = start; code <= end; code++) {
        const candidate = String.fromCodePoint(code);
        if (!used.has(candidate)) { literalLessThan = candidate; break; }
      }
      if (literalLessThan) break;
    }
    if (!literalLessThan) return null;
    const literal = (value) => {
      if (typeof value === "string") return value.replace(/</g, () => literalLessThan);
      if (Array.isArray(value)) return value.map(literal);
      if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([key, child]) => [key, literal(child)]));
      return value;
    };
    return request.items.map(({ ["ibis-rich-text"]: rich = {}, ...item }) => ({
      ...literal(typeof item.DOI === "string" ? { ...item, DOI: doiForUrl(item.DOI) } : item),
      ...Object.fromEntries(Object.entries(rich).map(([field, parts]) => [field,
        parts.map((part) => part.text === undefined ? part.markup : literal(part.text)).join("")
      ])),
    }));
  };
  // One request's records as the processor reads them, until they are closed.
  // The conformance run reads the suite's records through this same path.
  const items = Object.freeze({
    open: (request) => {
      const converted = processorItems(request);
      if (converted) activeItems = new Map(converted.map((item) => [item.id, item]));
      return converted;
    },
    close: () => {
      activeItems.clear();
      literalLessThan = null;
    },
  });
  globalThis.__ibisItems = items;
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
  // Numeric locators remain NumberBlobs until the processor's final range
  // pass. Serialize copies of those now-final leaves only; no cite tree is
  // evaluated again, and no processor blob changes.
  const leafText = (engine, part) => part.parts.map((leaf) => typeof leaf === "string" ? leaf : engine.output.renderBlobs([
    { ...leaf, strings: { ...leaf.strings }, decorations: [...leaf.decorations], checkNext: null, checkLast: null },
  ])).join("");
  // Every digit, or with none every letter, moved by one; 9 becomes 8 so a
  // range keeps its order. Numbering, labels and ranges keep their shape.
  const shiftedLocator = (value) => /[0-9]/.test(value)
    ? value.replace(/[0-9]/g, (digit) => String(digit === "9" ? 8 : Number(digit) + 1))
    : value.replace(/[A-Za-z]/g, (letter) => /[yzYZ]/.test(letter) ? String.fromCharCode(letter.charCodeAt(0) - 1) : String.fromCharCode(letter.charCodeAt(0) + 1));
  // The text each cite owns, without the cluster's shared punctuation.
  const ownedText = (engine, capture, count) => {
    const texts = Array(count).fill("");
    for (const part of capture.parts) if (part.item !== undefined && part.item !== null) texts[part.item] += leafText(engine, part);
    return texts;
  };
  // One processor, set up as Zotero sets up its own (Zotero.Style.getCiteProc):
  // URLs and DOIs are links in HTML, and names arrive already split by their
  // reader, so the processor never parses them again and a quoted "de Vries"
  // stays one family name.
  const newEngine = (request, style) => {
    const engine = new CSL.Engine({ retrieveLocale: __ibisLocaleXml, retrieveItem: (id) => activeItems.get(id), uppercase_subtitles: request.sentence_titles }, style, request.locale ?? undefined, request.locale !== null);
    engine.opt.development_extensions.throw_on_empty = true;
    engine.opt.development_extensions.wrap_url_and_doi = true;
    engine.opt.development_extensions.parse_names = false;
    return engine;
  };
  const oneLine = (text) => text.replace(/\s*\n\s*/g, " ").trim();
  const wrapRtf = (text, bibliography) => `{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0 Times New Roman;}}\\f0\n${bibliography ? text.split("\n").join("\\par\n") : text}\n}`;
  // One document's citations as one sequence (`ibis format`): its own
  // engine and unchanged style, so numbering, year letters and first and
  // subsequent citations follow the document's order. A narrative citation is
  // the author, then the rest with the author suppressed, as `cluster` writes it.
  const document = (request, source) => {
    if (!items.open(request)) return JSON.stringify({ status: "refused" });
    engines.clear();
    const engine = newEngine(request, source);
    try {
      const citations = [];
      const add = (citationItems) => {
        citations.push({ citationID: `ibis-${citations.length}`, citationItems, properties: { noteIndex: 0 } });
        return citations.length - 1;
      };
      const groups = request.clusters.map((indices) => {
        const cites = indices.map((index) => request.cites[index]);
        if (cites.some((cite) => !cite)) throw unverified;
        if (!(cites.length === 1 && cites[0].narrative)) return { cites, parts: [add(cites)] };
        const { narrative, prefix, suffix, locator, label, ...cite } = cites[0];
        return {
          cites,
          parts: [
            add([{ ...cite, prefix, "author-only": true, "suppress-author": false }]),
            add([{ ...cite, suffix, locator, label, "suppress-author": true, "author-only": false }]),
          ],
        };
      });
      const results = engine.rebuildProcessorState(citations, "html", []);
      if (engine.tmp.citation_errors.length) throw unverified;
      const written = new Map(results.map(([id, , text]) => [id, text]));
      const out = groups.map(({ cites, parts }) => {
        const owned = new Set(cites.flatMap((cite) => [cite.prefix, cite.suffix]).filter(Boolean));
        const html = parts.map((index) => {
          const text = written.get(citations[index].citationID);
          if (typeof text !== "string") throw unverified;
          return text;
        }).join(" ");
        return writeAffixes(engine, "html", html, (request.affixes ?? []).filter((affix) => owned.has(affix.placeholder)));
      });
      // A citation number, year, affix or another cite's page cannot prove
      // that a locator printed. Preview each cite at this same document
      // position with a different locator of the same shape, comparing only
      // the processor text that cite owns, as on the selection path below.
      const unprinted_locators = [];
      engine.setOutputFormat("text");
      for (const [groupIndex, { parts }] of groups.entries()) {
        for (const part of parts) {
          const list = citations[part].citationItems;
          if (!list.some((cite) => cite.locator !== undefined)) continue;
          const indices = new Map();
          list.forEach((cite, index) => {
            if (!indices.has(cite.id)) indices.set(cite.id, []);
            indices.get(cite.id).push(index);
          });
          const before = citations.slice(0, part).map((cite) => [cite.citationID, 0]);
          const after = citations.slice(part + 1).map((cite) => [cite.citationID, 0]);
          const owned = (list) => {
            const capture = { engine, format: "text", area: "citation", seen: new Map(), roots: new WeakMap(), parts: [], indices, occurrences: new Map() };
            observation = capture;
            try {
              engine.previewCitationCluster({ citationItems: list.map((cite) => ({ ...cite })), properties: { noteIndex: 0 } }, before, after, "text");
              if (engine.tmp.citation_errors.length) throw unverified;
              return ownedText(engine, capture, list.length);
            } finally {
              observation = null;
            }
          };
          const original = owned(list);
          list.forEach((cite, index) => {
            if (cite.locator === undefined) return;
            const { locator, label, ...bare } = cite;
            const other = shiftedLocator(locator);
            const changed = other === locator ? bare : { ...cite, locator: other };
            const altered = owned(list.map((cite, at) => at === index ? changed : cite));
            if (original[index] === altered[index]) unprinted_locators.push(request.clusters[groupIndex][index]);
          });
        }
      }
      engine.setOutputFormat("html");
      engine.output.checkNestedBrace = undefined;
      const bibliography = engine.makeBibliography();
      if (bibliography && bibliography[0].bibliography_errors.length) throw unverified;
      return JSON.stringify({
        status: "rendered",
        citations: out,
        entries: bibliography ? bibliography[1] : [],
        hanging_indent: Boolean(bibliography && bibliography[0].hangingindent),
        unprinted_locators,
      });
    } catch (error) {
      // A citation the style prints as nothing refuses the document, like any
      // citation Ibis cannot verify. Resource limits still surface as themselves.
      const refused = error === unverified || error instanceof TypeError
        || (error?.name === "Error" && error.message === "citation formatting failed");
      if (!refused) throw error;
      return JSON.stringify({ status: "refused" });
    } finally {
      observation = null;
      items.close();
    }
  };
  globalThis.__ibisFormat = (request, source) => {
    if (request.clusters) return document(request, source);
    const converted = items.open(request);
    if (!converted) return JSON.stringify({ status: "refused" });
    const key = JSON.stringify([request.style, request.locale, request.keep_order, request.sentence_titles]);
    let engine = engines.get(key);
    if (!engine) {
      // Keep one warm style, rather than eight independent item registries.
      engines.clear();
      let xml = source.replace(/disambiguate-add-year-suffix="true"/g, 'disambiguate-add-year-suffix="false"');
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
            const number = (child) => child.name === "text" && child.attrs.variable === "citation-number";
            // A number that opens the entry takes its separator with it: the
            // next element's leading ". " only ever followed the number.
            if (node.name === "layout" && node.children[0] && number(node.children[0]) && node.children[1]?.attrs?.prefix) {
              node.children[1].attrs.prefix = node.children[1].attrs.prefix.replace(/^[\s.,;:]+/, "");
            }
            node.children = node.children.filter((child) => !number(child));
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
      engine = newEngine(request, xml);
      if (request.keep_order) engine.citation_sort.tokens = [];
      engines.set(key, engine);
    }
    // Startup needs no references and publishes no citation. Its first real
    // request uses this same compiled style and locale with its own items.
    if (request.warm) return JSON.stringify({ status: "rendered", text: "", html: "", rtf: "", coverage: [], spans: [] });
    const outputs = { status: "rendered" };
    const indices = new Map();
    request.cites.forEach((cite, index) => indices.set(cite.id, [...indices.get(cite.id) ?? [], index]));
    let owned = null;
    const expected = new Map();
    for (const cite of request.cites) expected.set(cite.id, request.bibliography ? 1 : (expected.get(cite.id) ?? 0) + (cite.narrative ? 2 : 1));
    try {
      // Register and disambiguate the selected records once. The three output
      // modes serialize that same processor state, rather than rebuilding it.
      engine.setOutputFormat("text");
      engine.updateItems(request.items.map((item) => item.id));
      for (const format of ["text", "html", "rtf"]) {
        engine.setOutputFormat(format);
        const capture = { engine, format, area: request.bibliography ? "bibliography" : "citation", seen: new Map(), roots: new WeakMap(), parts: [], indices, occurrences: new Map() };
        observation = capture;
        let output;
        if (request.bibliography) {
          // The processor creates its nested-bracket flipper only while writing a
          // citation, then leaves it for the bibliography. Without this, a note
          // style's entries depend on what this engine last cited: "(2026]".
          engine.output.checkNestedBrace = undefined;
          const bibliography = engine.makeBibliography();
          if (!bibliography || bibliography[0].bibliography_errors.length) throw unverified;
          if (format === "text") outputs.text_parts = bibliography[1];
          if (format === "html") {
            // How the style lays entries out; the formatter writes it as Zotero does.
            const [layout] = bibliography;
            outputs.layout = {
              hanging_indent: Boolean(layout.hangingindent),
              line_spacing: layout.linespacing,
              entry_spacing: layout.entryspacing,
              second_field_align: layout["second-field-align"] || null,
              max_offset: layout.maxoffset,
            };
          }
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
            owned = ownedText(engine, capture, request.cites.length);
            const parts = capture.parts.map((part) => {
              const text = leafText(engine, part);
              return { item: part.item, text: writeAffixes(engine, format, text, (request.affixes ?? []).filter((affix) => text.includes(affix.placeholder))) };
            });
            outputs.spans = spans(output, parts);
          }
        }
        observation = null;
        if (capture.seen.size !== expected.size || [...expected].some(([id, count]) => capture.seen.get(id) !== count)) throw unverified;
        outputs[format] = format === "rtf" ? wrapRtf(output, request.bibliography) : output;
      }
      if (!request.bibliography) {
        // A style can print a locator for some types or forms only. The
        // evidence is the cite's own text: if it reads the same without the
        // locator, the locator was left out. Shared punctuation, which can
        // change when cites group differently, is not part of it.
        engine.setOutputFormat("text");
        outputs.unprinted_locators = request.cites.flatMap((cite, index) => {
          if (cite.locator === undefined) return [];
          // The same locator with another value of the same shape: if the
          // cite reads the same, its value was not printed. Leaving it out
          // instead would miss a style that prints the record's own page.
          const { locator, label, ...bare } = cite;
          const other = shiftedLocator(locator);
          const without = other === locator ? bare : { ...cite, locator: other };
          let same;
          const capture = { engine, format: "text", area: "citation", seen: new Map(), roots: new WeakMap(), parts: [], indices, occurrences: new Map() };
          observation = capture;
          try {
            cluster(engine, request.cites.map((other, at) => at === index ? without : other));
            same = ownedText(engine, capture, request.cites.length)[index] === owned[index];
          } catch (error) {
            const refused = error === unverified || error instanceof TypeError
              || (error?.name === "Error" && error.message === "citation formatting failed");
            if (!refused) throw error;
            // Nothing prints without it, so the locator printed. The outputs
            // above are final, but this engine is never reused.
            engines.delete(key);
            same = false;
          } finally {
            observation = null;
          }
          return same ? [index] : [];
        });
      }
    } catch (error) {
      // A partially evaluated engine must never be reused after a refusal.
      engines.delete(key);
      // A style the processor cannot run on this record refuses it, like any
      // citation Ibis cannot verify. Resource limits still surface as themselves.
      if (error !== unverified && !(error instanceof TypeError)) throw error;
      return JSON.stringify({ status: "refused" });
    } finally {
      observation = null;
      // No cached engine keeps the last request's source records alive.
      items.close();
      engine.updateItems([]);
    }
    outputs.coverage = request.cites.map((_, index) => index);
    return JSON.stringify(outputs);
  };
})();
