// Excerpted 2026-11-14 from Juris-M/zotero-odf-scan-plugin v3.2.2
// Revision 27a6ae72e6bb048fa808cd987f95327f67efd70f. See README.md and ../../LICENSE.
/*
    ***** BEGIN LICENSE BLOCK *****

    Copyright © 2009 Center for History and New Media
                     George Mason University, Fairfax, Virginia, USA
                     http://zotero.org

    This file is part of Zotero.

    Zotero is free software: you can redistribute it and/or modify
    it under the terms of the GNU Affero General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.
    Zotero is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Affero General Public License for more details.

    You should have received a copy of the GNU Affero General Public License
    along with Zotero.  If not, see <http://www.gnu.org/licenses/>.

    ***** END LICENSE BLOCK *****
*/
var CitationUtils = {
    log(msg) {
        Zotero.debug("CitationUtils: " + msg);
    },
// citationUtils.js lines 140-189
    async getItemByURI(uri) {
        try {
            let itemKey;
            let libraryID = Zotero.Libraries.userLibraryID;
            if (uri.startsWith('zotero://select/')) {
                if (uri.includes('/groups/')) {
                    const parts = uri.match(/\/groups\/(\d+)\/items\/(.+)/);
                    if (parts) {
                        const groupID = parseInt(parts[1]);
                        libraryID = Zotero.Groups.getLibraryIDFromGroupID(groupID);
                        itemKey = parts[2];
                    }
                } else {
                    itemKey = uri.split('/items/')[1];
                    if (itemKey && itemKey.includes('_')) {
                        itemKey = itemKey.split('_')[1];
                    }
                }
            } else if (uri.startsWith('zu:')) {
                itemKey = uri.split(':')[2];
            } else if (uri.includes('/items/')) {
                if (uri.includes('/groups/')) {
                    const parts = uri.match(/\/groups\/(\d+)\/items\/(.+)/);
                    if (parts) {
                        const groupID = parseInt(parts[1]);
                        libraryID = Zotero.Groups.getLibraryIDFromGroupID(groupID);
                        itemKey = parts[2];
                    }
                } else {
                    itemKey = uri.split('/items/')[1];
                }
            } else {
                CitationUtils.log("Warning: Unrecognized URI format: " + uri);
                return null;
            }
            if (!itemKey) {
                CitationUtils.log("Warning: Could not extract item key from URI: " + uri);
                return null;
            }

            CitationUtils.log("Looking up item with key: " + itemKey + " in library: " + libraryID);

            const item = await Zotero.Items.getByLibraryAndKeyAsync(libraryID, itemKey);
            return item;

        } catch (e) {
            CitationUtils.log("Error looking up item: " + e);
            return null;
        }
    },
// citationUtils.js lines 198-454
    async pandocToMarkersText(content) {
        CitationUtils.log("Converting pandoc citations to markers (ODF/text mode)");
        let convertedCount = 0;
        const notFoundKeys = [];
        // Helper: look up item and return { item, firstCreator, title, year, uri } or null
        // Uses the short zu:/zg: format that odfConverter.js markers require.
        const lookupItem = async (citekey) => {
            const item = await CitationUtils.findItemByCitationKey(citekey);
            if (!item) {
                CitationUtils.log(`Warning: Item not found for citationKey: ${citekey}`);
                notFoundKeys.push(citekey);
                return null;
            }
            const uri = await CitationUtils.buildItemURIShort(item);
            let firstCreator = '';
            let title = '';
            let year = '';
            try { firstCreator = (item.getField('firstCreator') || '').replace(/[\u2068\u2069]/g, ''); } catch(e) {}
            try { title = item.getField('title') || ''; } catch(e) {}
            try { year = item.getField('year') || ''; } catch(e) {
                try { year = (item.getField('date') || '').match(/\d{4}/)?.[0] || ''; } catch(e2) {}
            }
            return { item, firstCreator, title, year, uri };
        };
        // Pass 1: bracket groups  [@key; @key2, p. 3]
        // Collect all matches first, then process in reverse to preserve string indices.
        const bracketRegex = /\[([^\[\]]*@[^\[\]]*)\]/g;
        let bm;
        const bracketMatches = [];
        while ((bm = bracketRegex.exec(content)) !== null) {
            bracketMatches.push({ full: bm[0], inner: bm[1], index: bm.index });
        }
        for (let i = bracketMatches.length - 1; i >= 0; i--) {
            const { full, inner, index } = bracketMatches[i];
            const entries = CitationUtils.parsePandocCitationGroup(inner);
            const markers = [];
            for (const entry of entries) {
                const found = await lookupItem(entry.citekey);
                if (!found) continue;
                let cite = found.firstCreator;
                if (found.title) cite += (cite ? ', ' : '') + found.title;
                if (found.year) cite += ` (${found.year})`;
                if (entry.suppressAuthor) cite = '-' + cite;
                markers.push(`{ ${entry.prefix} | ${cite} | ${entry.locator} | ${entry.suffix} | ${found.uri}}`);
                convertedCount++;
            }
            if (markers.length) {
                content = content.substring(0, index) + markers.join('') + content.substring(index + full.length);
            }
        }
        // Pass 2: author-in-text  @key [loc]  or bare  @key
        // After pass 1 the bracket groups are gone, so all remaining @key are author-in-text.
        const aitRegex = /@([\w][\w:.#$%&\-+?<>~/]*)(?:\s*\[([^\]]*)\])?/g;
        let am;
        const aitMatches = [];
        while ((am = aitRegex.exec(content)) !== null) {
            aitMatches.push({ full: am[0], citekey: am[1], locatorText: am[2], index: am.index });
        }
        for (let i = aitMatches.length - 1; i >= 0; i--) {
            const { full, citekey, locatorText, index } = aitMatches[i];
            const found = await lookupItem(citekey);
            if (!found) continue;
            let cite = '-' + found.firstCreator;
            if (found.title) cite += ', ' + found.title;
            if (found.year) cite += ` (${found.year})`;
            const locatorInfo = locatorText ? CitationUtils.parsePandocLocator(locatorText.trim()) : { locator: '', suffix: '' };
            const marker = `{ | ${cite} | ${locatorInfo.locator} | ${locatorInfo.suffix} | ${found.uri}}`;
            convertedCount++;
            content = content.substring(0, index) + found.firstCreator + ' ' + marker + content.substring(index + full.length);
        }
        if (notFoundKeys.length > 0) {
            CitationUtils.log(`Warning: ${notFoundKeys.length} citation keys not found: ${notFoundKeys.join(', ')}`);
        }
        CitationUtils.log(`Converted ${convertedCount} pandoc citations to markers (ODF/text mode)`);
        return content;
    },
    /**
     * Locator prefix patterns shared by parsePandocLocator and the
     * curly-brace forced-locator handling in parseBracedLocator.
     */
    pandocLocatorPrefixes: [
        { pattern: /^pp?\.\s*|^pages?\s*/i,                          label: 'page',      display: 'p.' },
        { pattern: /^ch\.\s*|^chaps?\.\s*|^chapters?\s*/i,           label: 'chapter',   display: 'ch.' },
        { pattern: /^secs?\.\s*|^sections?\s*/i,                      label: 'section',   display: 'sec.' },
        { pattern: /^vols?\.\s*|^volumes?\s*/i,                       label: 'volume',    display: 'vol.' },
        { pattern: /^nos?\.\s*|^numbers?\s*/i,                        label: 'number',    display: 'no.' },
        { pattern: /^paras?\.\s*|^paragraphs?\s*/i,                   label: 'paragraph', display: 'para.' },
        { pattern: /^figs?\.\s*|^figures?\s*/i,                       label: 'figure',    display: 'fig.' },
        { pattern: /^ll?\.\s*/i,                                      label: 'line',      display: 'l.' },
        { pattern: /^nn?\.\s*|^notes?\s*/i,                           label: 'note',      display: 'n.' },
        { pattern: /^arts?\.\s*|^articles?\s*/i,                      label: 'article',   display: 'art.' },
    ],
    /**
     * Parse a pandoc citation group (the text inside [...])
     * Returns array of { citekey, prefix, locator, label, suffix, suppressAuthor }
     *
     * Supports pandoc's curly-brace locator syntax (see
     * https://pandoc.org/demo/example33/8.20-citation-syntax.html):
     *   @key{loc}      — force "loc" to be treated as the locator, even if it
     *                     doesn't match a recognized prefix (e.g. "ii, A, D-Z")
     *   @key{}         — suppress locator parsing entirely; everything after
     *                     the (optional) following comma is suffix, verbatim
     *   @key, {loc}    — same forcing, applied after the comma so a locator
     *                     containing commas/semicolons isn't split apart
     */
    parsePandocCitationGroup(text) {
        const entries = [];
        const parts = text.split(';');

        for (const part of parts) {
            const trimmed = part.trim();
            if (!trimmed) continue;

            const citeMatch = trimmed.match(/(-?)@([\w][\w:.#$%&\-+?<>~/]*)/);
            if (!citeMatch) continue;

            const suppressAuthor = citeMatch[1] === '-';
            const citekey = citeMatch[2];

            let prefix = trimmed.substring(0, citeMatch.index).trim();
            const afterKey = trimmed.substring(citeMatch.index + citeMatch[0].length).trim();

            let locator = '';
            let label = 'page';
            let suffix = '';
            if (afterKey.startsWith('{')) {
                const braced = CitationUtils.parseBracedLocator(afterKey);
                locator = braced.locator;
                label = braced.label;
                suffix = braced.suffix;
            } else if (afterKey.startsWith(',')) {
                const afterComma = afterKey.substring(1).trim();
                if (afterComma.startsWith('{')) {
                    const braced = CitationUtils.parseBracedLocator(afterComma);
                    locator = braced.locator;
                    label = braced.label;
                    suffix = braced.suffix;
                } else {
                    const locatorInfo = CitationUtils.parsePandocLocator(afterComma);
                    locator = locatorInfo.locator;
                    label = locatorInfo.label;
                    suffix = locatorInfo.suffix;
                }
            } else if (afterKey) {
                suffix = afterKey;
            }
            entries.push({ citekey, prefix, locator, label, suffix, suppressAuthor });
        }

        return entries;
    },
    /**
     * Parse pandoc's curly-brace forced-locator syntax. `text` must start
     * with '{'. Handles both the empty-braces "suppress" form (@key{}) and
     * the non-empty "force" form (@key{loc} or the post-comma {loc} form).
     * Returns { locator, label, suffix }.
     */
    parseBracedLocator(text) {
        let depth = 0;
        let closeIdx = -1;
        for (let i = 0; i < text.length; i++) {
            if (text[i] === '{') depth++;
            else if (text[i] === '}') {
                depth--;
                if (depth === 0) { closeIdx = i; break; }
            }
        }
        if (closeIdx === -1) {
            // Unbalanced braces — fall back to treating the whole thing as suffix
            return { locator: '', label: 'page', suffix: text.trim() };
        }

        const bracesContent = text.substring(1, closeIdx).trim();
        let remainder = text.substring(closeIdx + 1).trim();
        if (remainder.startsWith(',')) remainder = remainder.substring(1).trim();
        if (!bracesContent) {
            // {} — suppress locator parsing; remainder is suffix, verbatim
            return { locator: '', label: 'page', suffix: remainder };
        }
        // Non-empty braces — force this text to be treated as a locator,
        // still detecting a recognized label prefix (e.g. "pp.") if present.
        for (const lp of CitationUtils.pandocLocatorPrefixes) {
            const m = bracesContent.match(lp.pattern);
            if (m) {
                const rest = bracesContent.substring(m[0].length).trim();
                return {
                    locator: rest ? lp.display + ' ' + rest : lp.display,
                    label: lp.label,
                    suffix: remainder
                };
            }
        }
        return { locator: bracesContent, label: 'page', suffix: remainder };
    },
    /**
     * Parse a pandoc locator string (text after citekey comma).
     * Recognizes: p., pp., ch., chap., chapter, sec., vol., etc.
     * Returns { locator, label, suffix } where label is the CSL term
     * (e.g. "chapter") and locator is the display string (e.g. "ch. 3").
     */
    parsePandocLocator(text) {
        for (const lp of CitationUtils.pandocLocatorPrefixes) {
            const m = text.match(lp.pattern);
            if (m) {
                const rest = text.substring(m[0].length);
                const valueMatch = rest.match(/^([\d\-–—,\s]+)(.*)/);
                if (valueMatch) {
                    return {
                        locator: lp.display + ' ' + valueMatch[1].trim(),
                        label: lp.label,
                        suffix: valueMatch[2].trim()
                    };
                }
                return { locator: lp.display + ' ' + rest.trim(), label: lp.label, suffix: '' };
            }
        }
        // No recognized prefix — if starts with digit, assume page
        const digitMatch = text.match(/^([\d\-–—,\s]+)(.*)/);
        if (digitMatch) {
            return {
                locator: 'p. ' + digitMatch[1].trim(),
                label: 'page',
                suffix: digitMatch[2].trim()
            };
        }

        // No locator found, treat everything as suffix
        return { locator: '', label: 'page', suffix: text.trim() };
    },
    /**
     * Search Zotero for an item by its citationKey field
     */
    async findItemByCitationKey(citationKey) {
        try {
            CitationUtils.log("Searching for citationKey: " + citationKey);

            const s = new Zotero.Search();
            s.libraryID = Zotero.Libraries.userLibraryID;
            s.addCondition('citationKey', 'is', citationKey);
            const ids = await s.search();
            if (ids.length === 0) {
                CitationUtils.log("No item found for citationKey: " + citationKey);
                return null;
            }

            if (ids.length > 1) {
                CitationUtils.log(`Warning: Multiple items found for citationKey: ${citationKey}, using first`);
            }

            const item = await Zotero.Items.getAsync(ids[0]);
            CitationUtils.log(`Found item: ${item.getField('title')} (key: ${item.key})`);
            return item;
        } catch (e) {
            CitationUtils.log("Error searching for citationKey: " + e);
            return null;
        }
    },
// citationUtils.js lines 476-487
    async buildItemURIShort(item) {
        const libraryID = item.libraryID;
        const library = Zotero.Libraries.get(libraryID);
        if (library.libraryType === 'user') {
            const userID = Zotero.Users.getCurrentUserID();
            const lib = userID ? String(userID) : '0';
            return `zu:${lib}:${item.key}`;
        } else {
            const groupID = Zotero.Groups.getGroupIDFromLibraryID(libraryID);
            return `zg:${groupID}:${item.key}`;
        }
    },
};
