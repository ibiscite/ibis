# Native picker pictures

Captured on macOS 27.2.1 from the repo app with an invented CSL-JSON library and a disposable TextEdit document. The external display supplied 1× and the built-in Retina display supplied 2×. Each PNG came from `screencapture -l` with the window shadow excluded. The app was set to APA. The two result densities, the details drawer and the Larger Text captures highlight the second reference.

| State | Light 1× | Dark 1× | Light 2× | Dark 2× |
| --- | --- | --- | --- | --- |
| Compact results | [light-1x](results-compact-light-1x.png) | [dark-1x](results-compact-dark-1x.png) | [light-2x](results-compact-light-2x.png) | [dark-2x](results-compact-dark-2x.png) |
| Comfortable results | [light-1x](results-comfortable-light-1x.png) | [dark-1x](results-comfortable-dark-1x.png) | [light-2x](results-comfortable-light-2x.png) | [dark-2x](results-comfortable-dark-2x.png) |
| Empty search | [light-1x](empty-search-light-1x.png) | [dark-1x](empty-search-dark-1x.png) | [light-2x](empty-search-light-2x.png) | [dark-2x](empty-search-dark-2x.png) |
| No results | [light-1x](no-results-light-1x.png) | [dark-1x](no-results-dark-1x.png) | [light-2x](no-results-light-2x.png) | [dark-2x](no-results-dark-2x.png) |
| Details drawer | [light-1x](details-light-1x.png) | [dark-1x](details-dark-1x.png) | [light-2x](details-light-2x.png) | [dark-2x](details-dark-2x.png) |
| Larger Text at 150% | [light-1x](larger-text-light-1x.png) | [dark-1x](larger-text-dark-1x.png) | [light-2x](larger-text-light-2x.png) | [dark-2x](larger-text-dark-2x.png) |

The standard window is 720 × 480 pt. Larger Text scales it to 1080 × 720 pt. Retina captures contain twice the pixels per point in both dimensions.

## Differences from the web picker

The supplied dark audit pictures are the references for results and empty search. The [web gallery](../../../apps/desktop/gallery/baselines/) supplies the matching light and dark states for each comparison, including no matches, citation details and Larger Text.

1. **Window**. The native capture shows a borderless 16 pt rounded surface. AppKit glass in Light reads close to white. Dark is darker than the audit’s charcoal. The panel kept its 720 × 480 pt frame through typing and opening the drawer. A right-edge drag changed the width from 720 to 796 pt. The tint is 70% of the appearance’s window background. Light, dark and patterned test backdrops were supplied by disposable windows.
2. **Search**. The bird, text and format keep the original alignment. The AppKit text cell adds a small inset inside its column. The placeholder reads “Search references” and the trailing APA choice uses the native paired chevrons.
3. **Results**. Compact holds one line per row and aligns the byline column. Comfortable holds two lines per row. The original highlighted row was taller and swapped its kind for a plus. The captured native row keeps its kind and has a trailing check. The native list drops a container whole when it runs out of width. Empty search fills the available rows from recent additions. The no-results screen uses text, Clear Search and Open Zotero without the web gallery’s magnifier.
4. **Preview and details**. The details drawer is on the title grid under the list. It leads with the full invented title and byline. Show as and the locator sit in one strip. The entered page 417 is in the same citation preview Return delivers. The web drawer had separate labelled form rows.
5. **Bar**. The beak, citation, library, action and More keep their order. Native hover capsules and keycaps have system fills. The captures show Copy under the test app’s paste permissions. The audit target was Claude and its action was Paste into Claude.
6. **Accessibility**. Larger Text increases the whole layout, including the window, while keeping row and control alignment. The gallery’s larger-text example enlarges content within its original window size. The AppKit test checks 200% row height, scrolling and footer fit. VoiceOver speech and accessibility appearance overrides still need a device check.
7. **Visual constraints**. The title bar, magnifier, expanding selection and swapped kind glyphs are absent. The figures use the genuine window captures and brand exports.
