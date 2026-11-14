# Offline ODF Scan oracle

Source: [Juris-M/zotero-odf-scan-plugin v3.2.2](https://github.com/Juris-M/zotero-odf-scan-plugin/releases/tag/v3.2.2),
commit [`27a6ae72e6bb048fa808cd987f95327f67efd70f`](https://github.com/Juris-M/zotero-odf-scan-plugin/commit/27a6ae72e6bb048fa808cd987f95327f67efd70f).
Retrieved and excerpted 2026-11-14. Ordinary tests never fetch upstream.

The two JavaScript fixtures retain verbatim conversion methods and declarations;
comments identify their original, one-based line ranges. Only the enclosing
objects were reduced: `CitationUtils` contains logging, URI lookup, the Pandoc
text converter and its parsing/search/URI helpers; `ODFConv` contains its citation
template, marker/styling expressions, composition, markup and random-ID methods.
The original file licence blocks are retained. ZIP I/O, UI, reverse conversion
and the DOCX converter are omitted. No replacement citation scanner is authored.

For comparison or refresh, use `chrome/content/citationUtils.js` and
`chrome/content/odfConverter.js` at the pinned commit. Compare the recorded
ranges before changing the excerpts; retain their notices. The harness supplies
only invented Zotero API records and an XML paragraph, then reads emitted
reference-mark JSON and calls upstream URI lookup. It uses upstream's own marker
expression to avoid calling composition on marker-free text.

`scenarios.json` records human-readable intent, independently of Ibis's marker
spelling: expected item identity, locator text, suppression, affix markup and
citation grouping. Personal and group records deliberately share an item key;
the wrong-key decoy shares the title. Defaults mean no locator/affix/suppression.
These expectations specify the intended result, independently of Ibis output.
The Rust test supplies all citation strings through `Engine`.
The shared-key group case also substitutes a personal-library identifier in the
engine output and requires the comparison to reject that resolved identity.
Run the verify command in [the guide](../../docs/live-citations.md#evidence-and-remaining-native-checks)
to execute both Rust tests, their oracle comparisons and this negative control.

Upstream distribution notice (from `COPYING.txt`):

RTF/ODF Scan for Zotero is Copyright © 2013-2026 Frank G. Bennett, Jr., Faculty of
Law, Nagoya University and Sebastian Karcher https://orcid.org/0000-0001-8249-7388.
The author distributes the source code under the GNU Affero General Public
License, version 3 (AGPLv3). Third-party copyright in this distribution is noted
where applicable. The Zotero name is a registered trademark of George Mason
University. See http://zotero.org/trademark for more information.
All rights not expressly granted are reserved.

The licence text is included in [LICENSE](../../LICENSE). The retained source
blocks also identify the Center for History and New Media, George Mason
University copyright, the AGPL-3.0-or-later terms and absence of warranty.
