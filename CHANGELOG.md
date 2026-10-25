# Changelog

## [0.1.1](https://github.com/ibiscite/ibis/compare/v0.1.0...v0.1.1) (2026-10-20)

First-run setup finds Zotero and nearby `.bib` files and shows the shortcut, searches stay responsive on large libraries, copy format choices are kept, and the picker no longer overflows at large text sizes.


### Features

* ask where you write in first-run setup and show the shortcut ([#449](https://github.com/ibiscite/ibis/issues/449)) ([0a2f532](https://github.com/ibiscite/ibis/commit/0a2f5329e9cd5c2eb9c577f5d33a07e8db27c7e2))
* find Zotero and nearby .bib files for first-run setup ([#427](https://github.com/ibiscite/ibis/issues/427)) ([3a7a4d9](https://github.com/ibiscite/ibis/commit/3a7a4d9017626b09c5414f11f5a4891c925dfacd))
* offer Zotero and found .bib files in first-run setup ([#434](https://github.com/ibiscite/ibis/issues/434)) ([fdff870](https://github.com/ibiscite/ibis/commit/fdff8707d9879c93c653ce257b6d6f13a5a9505e))
* show the version on the nightly release and in the menu ([#435](https://github.com/ibiscite/ibis/issues/435)) ([26a1e06](https://github.com/ibiscite/ibis/commit/26a1e0669526c2e30aba305a6a78f139558bd252))


### Bug Fixes

* keep copy format choices when quitting during settings loading ([#468](https://github.com/ibiscite/ibis/issues/468)) ([62f0749](https://github.com/ibiscite/ibis/commit/62f0749ce51db31a9a9efddebc637305aae18e79))
* keep macOS fullscreen observation within its wait budget ([#481](https://github.com/ibiscite/ibis/issues/481)) ([93eb5eb](https://github.com/ibiscite/ibis/commit/93eb5ebc72461722938556b48d89a08da497b806))
* keep the picker within the window at large text sizes ([#462](https://github.com/ibiscite/ibis/issues/462)) ([dfc47aa](https://github.com/ibiscite/ibis/commit/dfc47aa0693d85ecc739984560fa516307c65eb5))
* keep Zotero groups after a library file fails to open ([#467](https://github.com/ibiscite/ibis/issues/467)) ([35a2eae](https://github.com/ibiscite/ibis/commit/35a2eae71122573cc612cfcdac119bc671b589ec))
* log the outcome of each automatic update check ([#456](https://github.com/ibiscite/ibis/issues/456)) ([4144d29](https://github.com/ibiscite/ibis/commit/4144d2935b3e5992c9a0e46a3e5c5943c7e27b4b)), closes [#319](https://github.com/ibiscite/ibis/issues/319)
* preserve explicit BibLaTeX name particles ([#426](https://github.com/ibiscite/ibis/issues/426)) ([5f22535](https://github.com/ibiscite/ibis/commit/5f22535c58246925e7140bcba006c76aced7f05b))
* refuse a Zotero group list that repeats a group, and strip invisible characters from group names ([#440](https://github.com/ibiscite/ibis/issues/440)) ([d66b3c7](https://github.com/ibiscite/ibis/commit/d66b3c7301ba92114e9357ddd7f8faf834f7ace1))
* save a key copy format as soon as it is chosen ([#463](https://github.com/ibiscite/ibis/issues/463)) ([4fed4e3](https://github.com/ibiscite/ibis/commit/4fed4e3fa97195e499afb6c29cf87eae711637be))


### Performance

* keep searches responsive on large libraries ([#388](https://github.com/ibiscite/ibis/issues/388)) ([f1e98a4](https://github.com/ibiscite/ibis/commit/f1e98a40dc7d99b9f178e22e9844b79f16b614fb)), closes [#364](https://github.com/ibiscite/ibis/issues/364)


### Documentation

* clarify picker design targets and rollout ([#460](https://github.com/ibiscite/ibis/issues/460)) ([a066cc8](https://github.com/ibiscite/ibis/commit/a066cc8b4aca5df96b41437d2671145b8998b970))
* consolidate contributor rules and testing guidance ([#428](https://github.com/ibiscite/ibis/issues/428)) ([06494e8](https://github.com/ibiscite/ibis/commit/06494e8181be10bace68d4c3c5227e233c1daeec))
* describe 0.1 by what shipped in 0.1.0 and narrow 0.2 to what is left ([#433](https://github.com/ibiscite/ibis/issues/433)) ([bfc618d](https://github.com/ibiscite/ibis/commit/bfc618d8f39d4f84d5bbe5211e899f3e0441fc0d))
* explain releases, nightly and version numbers in the README ([#442](https://github.com/ibiscite/ibis/issues/442)) ([c8a130e](https://github.com/ibiscite/ibis/commit/c8a130ed8acf9ff12f7d734cdf7141f6d9ae139a))
* explain Windows signing preparation and approval roles ([#459](https://github.com/ibiscite/ibis/issues/459)) ([05aa5e9](https://github.com/ibiscite/ibis/commit/05aa5e9619c60ecec34636ae363f4306788048aa))
* show meaningful interface changes with before and after screenshots ([#466](https://github.com/ibiscite/ibis/issues/466)) ([1b62245](https://github.com/ibiscite/ibis/commit/1b62245601f625f436f4671d34ef789863f3b415))
* write down one standard for labels, milestones and titles ([#439](https://github.com/ibiscite/ibis/issues/439)) ([47dfc60](https://github.com/ibiscite/ibis/commit/47dfc6096bb832795a97cf2098df3dbd84264bc3))

## 0.1.0 (2026-10-20)

The first preview. Press the shortcut anywhere, search a `.bib` file or your Zotero library (personal and group libraries, while Zotero is open), and copy a citation key or a formatted citation in one of five styles. Ibis works offline and never changes your library. The apps are not signed by Apple or Microsoft yet, so your system may warn before opening them; in-app updates are opt-in and signed.


### Features

* add a synthetic picker states gallery ([#221](https://github.com/ibiscite/ibis/issues/221)) ([75111f6](https://github.com/ibiscite/ibis/commit/75111f6b27bc0b03c0d3154f39111af5ffec7907)), closes [#220](https://github.com/ibiscite/ibis/issues/220)
* add native material groundwork with opaque fallbacks ([#232](https://github.com/ibiscite/ibis/issues/232)) ([f3ec5db](https://github.com/ibiscite/ibis/commit/f3ec5db3db4975746fb5ad337bb7de2247420c07))
* add opt-in, signed in-app update checks ([#316](https://github.com/ibiscite/ibis/issues/316)) ([2fb9e35](https://github.com/ibiscite/ibis/commit/2fb9e35ec9126a7e2da4cea7a273f261b9eb05ee)), closes [#315](https://github.com/ibiscite/ibis/issues/315)
* add the desktop app scaffold (Tauri 2, React, TypeScript) ([#72](https://github.com/ibiscite/ibis/issues/72)) ([0660601](https://github.com/ibiscite/ibis/commit/0660601eb57d247a67920138b9cc2b6f449839ec))
* add the picker design scale as tokens ([#307](https://github.com/ibiscite/ibis/issues/307)) ([bf12015](https://github.com/ibiscite/ibis/commit/bf1201552bc215f643e599632995b6bcd26e5f79))
* choose the library from the picker or the menu bar ([#76](https://github.com/ibiscite/ibis/issues/76)) ([0084d95](https://github.com/ibiscite/ibis/commit/0084d95f25a1758adbda34e0d041c8539cf21c58))
* choose Zotero group libraries in the picker ([#422](https://github.com/ibiscite/ibis/issues/422)) ([6cf9541](https://github.com/ibiscite/ibis/commit/6cf9541d3767d15f6e4687addc0aa0aaa82caea2))
* default the macOS picker shortcut to Control+Command+R ([#188](https://github.com/ibiscite/ibis/issues/188)) ([8370626](https://github.com/ibiscite/ibis/commit/83706266a5f28e75cef5f1159ceb3370c9a5274b))
* format citations with pinned citeproc-js and five styles ([#286](https://github.com/ibiscite/ibis/issues/286)) ([3103747](https://github.com/ibiscite/ibis/commit/310374742880bfcb79c7f014872a562869825e5c))
* give the picker a launcher layout ([#91](https://github.com/ibiscite/ibis/issues/91)) ([978221a](https://github.com/ibiscite/ibis/commit/978221ac401e3fdb0277b5d96fde0af1b85a0dac))
* initial core library, command line tool and project foundation ([7135465](https://github.com/ibiscite/ibis/commit/7135465eb59a5fa08e341fcdee7212022df9b683))
* lay out the picker as designed ([#313](https://github.com/ibiscite/ibis/issues/313)) ([0917852](https://github.com/ibiscite/ibis/commit/091785256ec6adf6be56b122021494bb476b9e1c))
* map Zotero items to references ([#259](https://github.com/ibiscite/ibis/issues/259)) ([4830081](https://github.com/ibiscite/ibis/commit/48300811c792f0cec0cc9d0101a5e0ad3bbf67d8))
* pin the picker, step back with Escape and keep the search a minute ([#314](https://github.com/ibiscite/ibis/issues/314)) ([86e393c](https://github.com/ibiscite/ibis/commit/86e393c2e10baeda4aedf9a0d460797b719805f6))
* preview and copy formatted citations from the picker ([#302](https://github.com/ibiscite/ibis/issues/302)) ([76f9c13](https://github.com/ibiscite/ibis/commit/76f9c134a7b159e52e95fd23140c7ff62f12fcc1))
* read a Zotero library through the local API scan rules ([#260](https://github.com/ibiscite/ibis/issues/260)) ([5ba15d7](https://github.com/ibiscite/ibis/commit/5ba15d73a7d582673667d8cd30f2d5b456829b0b))
* read Zotero group libraries alongside the personal library ([#420](https://github.com/ibiscite/ibis/issues/420)) ([d35a7a7](https://github.com/ibiscite/ibis/commit/d35a7a78264294d3e3febfa86352c34099a622ae))
* reload watched BibTeX libraries safely ([#94](https://github.com/ibiscite/ibis/issues/94)) ([24de1a3](https://github.com/ibiscite/ibis/commit/24de1a3b82a2178985d2e34d8743371e74085e2c))
* remember citation style, density and text size in settings ([#407](https://github.com/ibiscite/ibis/issues/407)) ([f08a08b](https://github.com/ibiscite/ibis/commit/f08a08befdffc9b71122b9c44618d5eab634fc9f))
* remember the picker's citation style, rows and text size ([#408](https://github.com/ibiscite/ibis/issues/408)) ([9d68ca9](https://github.com/ibiscite/ibis/commit/9d68ca9e2e833b10c47e6d87cd267f458ea29a53))
* select references in the picker and copy them as a citation ([#75](https://github.com/ibiscite/ibis/issues/75)) ([8b8d9c8](https://github.com/ibiscite/ibis/commit/8b8d9c8031f508f80663f0ef881af326cecc223b))
* send feedback from the menu bar or tray menu ([#288](https://github.com/ibiscite/ibis/issues/288)) ([bf27238](https://github.com/ibiscite/ibis/commit/bf27238fda8fde5fecf1089ca61f7df26981d14a)), closes [#287](https://github.com/ibiscite/ibis/issues/287)
* settings format 2 names the library kind ([#261](https://github.com/ibiscite/ibis/issues/261)) ([47ebed7](https://github.com/ibiscite/ibis/commit/47ebed750adab2f0d8f0c6dc0d78ba7d42eea593))
* show the picker with a configurable global shortcut, single instance ([#73](https://github.com/ibiscite/ibis/issues/73)) ([47d177c](https://github.com/ibiscite/ibis/commit/47d177cfc708327c85156f9f50f9f7ae70f92de9))
* talk to Zotero's local API on this computer only ([#264](https://github.com/ibiscite/ibis/issues/264)) ([34e2d7f](https://github.com/ibiscite/ibis/commit/34e2d7fa4afb35e69f9a7309324e17c3dd1b58ec))
* use the Zotero library from the picker ([#268](https://github.com/ibiscite/ibis/issues/268)) ([956a490](https://github.com/ibiscite/ibis/commit/956a490b02fe1fa3e7ed6cd2434af41dba172081))


### Bug Fixes

* avoid quadratic BibTeX field-name lookups ([#152](https://github.com/ibiscite/ibis/issues/152)) ([3aac89f](https://github.com/ibiscite/ibis/commit/3aac89f4d8038b6911e52a10560ac4a73d062dd1))
* bound aggregate BibTeX load work ([#154](https://github.com/ibiscite/ibis/issues/154)) ([6d54f16](https://github.com/ibiscite/ibis/commit/6d54f160179169509245b410e841ee4355937b59))
* cache BibTeX abbreviations across entries ([#151](https://github.com/ibiscite/ibis/issues/151)) ([2c367ac](https://github.com/ibiscite/ibis/commit/2c367ac8ddfbacb299c7ece2130e545496f5d322))
* check Zotero for changes without flashing "Reloading" ([#282](https://github.com/ibiscite/ibis/issues/282)) ([8a9eea2](https://github.com/ibiscite/ibis/commit/8a9eea21804e47e9d7fe15f4984be27761b71daa))
* classify English language tags without case sensitivity ([#382](https://github.com/ibiscite/ibis/issues/382)) ([0483f72](https://github.com/ibiscite/ibis/commit/0483f72dc2c64520460dd5015fb05f8f53350b96))
* clip native macOS picker corners ([#248](https://github.com/ibiscite/ibis/issues/248)) ([36226e3](https://github.com/ibiscite/ibis/commit/36226e31fe2af57d1bfe8747a1955a8a267292ff)), closes [#236](https://github.com/ibiscite/ibis/issues/236)
* default to Ctrl+Shift+Y on Windows and Linux ([#252](https://github.com/ibiscite/ibis/issues/252)) ([3fbfa16](https://github.com/ibiscite/ibis/commit/3fbfa1689bbc5a64e49f18ca49246a6e22b49f28))
* explain libraries with no references ([efbe396](https://github.com/ibiscite/ibis/commit/efbe3963235fe4e1d1faee53dcd67c1a010660b5))
* explain library opening and reloading in the picker ([#204](https://github.com/ibiscite/ibis/issues/204)) ([02af8bc](https://github.com/ibiscite/ibis/commit/02af8bc028bbc69152b7a0ecc5e7e490f15b7968))
* explain settings recovery without overwriting the file ([ffc5d3a](https://github.com/ibiscite/ibis/commit/ffc5d3ade083d8879d41b9e76fa880e6f9ed2597))
* fold search text correctly in non-Latin scripts ([#379](https://github.com/ibiscite/ibis/issues/379)) ([d24ae9b](https://github.com/ibiscite/ibis/commit/d24ae9b3dacb47e4d4ade77311de762f930a2a34))
* guard unbalanced resolved BibTeX values ([#150](https://github.com/ibiscite/ibis/issues/150)) ([d12d5c3](https://github.com/ibiscite/ibis/commit/d12d5c3c6bff2264e6f93059809b64e6899c6178))
* keep caught parser panics private ([#172](https://github.com/ibiscite/ibis/issues/172)) ([f26ed93](https://github.com/ibiscite/ibis/commit/f26ed930b6bfa827b98b438cea1b89840a62a331))
* keep first-run search calm and announce result counts ([#180](https://github.com/ibiscite/ibis/issues/180)) ([0226209](https://github.com/ibiscite/ibis/commit/02262096742e17d35c9be5bcab0e03112735f322))
* keep initial library errors truthful about retained references ([#241](https://github.com/ibiscite/ibis/issues/241)) ([eccdc06](https://github.com/ibiscite/ibis/commit/eccdc0625a9952684701b4cc12ba9f4df270e482))
* keep startup diagnostics private and fallible ([#278](https://github.com/ibiscite/ibis/issues/278)) ([cfd919b](https://github.com/ibiscite/ibis/commit/cfd919b7b5066c5843874335b4c90fda6a2be7b8)), closes [#276](https://github.com/ibiscite/ibis/issues/276)
* keep the picker open while its file dialog is in front ([#87](https://github.com/ibiscite/ibis/issues/87)) ([7a59cb6](https://github.com/ibiscite/ibis/commit/7a59cb6694b9b24293f8bf25727efb7a190fa263))
* keep the Windows picker open when focus returns right after a blur ([#385](https://github.com/ibiscite/ibis/issues/385)) ([3f820a0](https://github.com/ibiscite/ibis/commit/3f820a0153f2e8c917d84e01f67aa6961819d88b))
* keep Typst citation notes as literal text ([#381](https://github.com/ibiscite/ibis/issues/381)) ([b5c2c46](https://github.com/ibiscite/ibis/commit/b5c2c46f9ec1ae3461d467d97828ede3b0b9a478))
* keep Windows WebView2 offline while Ibis runs ([#203](https://github.com/ibiscite/ibis/issues/203)) ([222ae9c](https://github.com/ibiscite/ibis/commit/222ae9c83c5e584e810405621dda0e8c7c9f6568))
* name unavailable libraries and show one recovery message ([0fa071a](https://github.com/ibiscite/ibis/commit/0fa071a12931b67443677e59bc7b66294604067c))
* open the picker on the display with the pointer ([#202](https://github.com/ibiscite/ibis/issues/202)) ([c80d3c3](https://github.com/ibiscite/ibis/commit/c80d3c3e7685d1426a83c1a72e3b458cdd8f08f4))
* preserve combining marks in Pandoc citation keys ([#380](https://github.com/ibiscite/ibis/issues/380)) ([479b150](https://github.com/ibiscite/ibis/commit/479b150f4abd7e95fc6fbdce083f0c0578fa7c0c))
* preserve failed library choices during automatic updates ([#258](https://github.com/ibiscite/ibis/issues/258)) ([08ad683](https://github.com/ibiscite/ibis/commit/08ad68390a8c6564808628faabd597c3bafe2778))
* preserve literal backslashes across text conversion ([#96](https://github.com/ibiscite/ibis/issues/96)) ([8697879](https://github.com/ibiscite/ibis/commit/869787905eedd21ddd49a7d38344ba72d08cc594)), closes [#90](https://github.com/ibiscite/ibis/issues/90)
* preserve positional BibTeX string definitions ([#386](https://github.com/ibiscite/ibis/issues/386)) ([0c412b5](https://github.com/ibiscite/ibis/commit/0c412b5e53217f86009a1b130e5d7864204a32bb))
* preserve Typst notes and unusual citation keys ([5744d9f](https://github.com/ibiscite/ibis/commit/5744d9f7d8c595aa1b2feeae2d031b683fbd247d))
* prevent deeply nested fields from overflowing the stack ([#146](https://github.com/ibiscite/ibis/issues/146)) ([cf5d3ca](https://github.com/ibiscite/ibis/commit/cf5d3ca07e0d64be172cb29061b793a3e5bef096))
* read Zotero items whose CSL JSON arrives as export text ([#297](https://github.com/ibiscite/ibis/issues/297)) ([785d6cf](https://github.com/ibiscite/ibis/commit/785d6cf3a9e32843e5d42f5557030ee4f959c9c8))
* recover incomplete BibTeX entries in linear time ([#147](https://github.com/ibiscite/ibis/issues/147)) ([afa33e6](https://github.com/ibiscite/ibis/commit/afa33e6665548fee996e420ee4f3d592c9b60c9b))
* recover library updates after long file locks ([52d5f42](https://github.com/ibiscite/ibis/commit/52d5f4206e0d9d5bf406f3a27bc3788c233697c1))
* recover missing symlink library targets automatically ([#104](https://github.com/ibiscite/ibis/issues/104)) ([3fcdf66](https://github.com/ibiscite/ibis/commit/3fcdf66e5f42a79006836a852679c6863828b5e0)), closes [#93](https://github.com/ibiscite/ibis/issues/93)
* recover transient library access and stabilize paths ([#173](https://github.com/ibiscite/ibis/issues/173)) ([a059075](https://github.com/ibiscite/ibis/commit/a059075b0aea1b2404d948784603878473f7dcda))
* replace links and the app identifier that use the old organization name ([#121](https://github.com/ibiscite/ibis/issues/121)) ([062861f](https://github.com/ibiscite/ibis/commit/062861ff894e701629eb5562e9b7c4d793b530a4))
* report native desktop failures without stderr panics ([#183](https://github.com/ibiscite/ibis/issues/183)) ([0a33623](https://github.com/ibiscite/ibis/commit/0a3362349eacb0c4348599bbb7446e670e41e1f6))
* report picker command failures and narrow web permissions ([#185](https://github.com/ibiscite/ibis/issues/185)) ([9c8dd02](https://github.com/ibiscite/ibis/commit/9c8dd02a7f6966e63c13ba49a746325ab2baf720))
* report unavailable pointer position once per process ([#218](https://github.com/ibiscite/ibis/issues/218)) ([efd6965](https://github.com/ibiscite/ibis/commit/efd6965dbb7b612f4b2f82451bf8e73665aefd8d))
* report warnings in all JSON output and make terminal output safe ([#71](https://github.com/ibiscite/ibis/issues/71)) ([341839d](https://github.com/ibiscite/ibis/commit/341839d18d9bb31060bfa1a8cf20f71cc2df5a10))
* report when the desktop cannot register the shortcut ([#237](https://github.com/ibiscite/ibis/issues/237)) ([2765abb](https://github.com/ibiscite/ibis/commit/2765abbd7d08f59ba2b6ec54375be6f2509e18a0)), closes [#235](https://github.com/ibiscite/ibis/issues/235)
* resolve BibTeX imports as a checked graph and preserve citation meaning ([#63](https://github.com/ibiscite/ibis/issues/63)) ([e644293](https://github.com/ibiscite/ibis/commit/e6442931f7129933fa5874a0bb9ac4434d643c36))
* restore the desktop build ([a8649ac](https://github.com/ibiscite/ibis/commit/a8649acf33c639dd4c507098584db44c34dba5a2))
* retain case-distinct xdata parents ([#384](https://github.com/ibiscite/ibis/issues/384)) ([4091299](https://github.com/ibiscite/ibis/commit/4091299bb4e32926c423324fdbe58572a2da8aa2))
* retain references with malformed date components ([#165](https://github.com/ibiscite/ibis/issues/165)) ([6576d8c](https://github.com/ibiscite/ibis/commit/6576d8c33a6d771c57a6b6e193b7e29846d01001))
* retain references with overflowing uncertain dates ([#115](https://github.com/ibiscite/ibis/issues/115)) ([44e0899](https://github.com/ibiscite/ibis/commit/44e0899d7b80bb78988cb99fdec77af5955266b6))
* retain references with unsafe separate date parts ([#117](https://github.com/ibiscite/ibis/issues/117)) ([e7450ee](https://github.com/ibiscite/ibis/commit/e7450ee0655ce9d12d01231f9799f1173ab542f1)), closes [#113](https://github.com/ibiscite/ibis/issues/113)
* retry briefly when WebView2 still holds a gallery profile ([#317](https://github.com/ibiscite/ibis/issues/317)) ([0d6d678](https://github.com/ibiscite/ibis/commit/0d6d6781e70c50e4a3e5551add309e8a264192f4))
* return application focus when the picker hides ([#206](https://github.com/ibiscite/ibis/issues/206)) ([2153523](https://github.com/ibiscite/ibis/commit/21535237fa283b5fa508a0bee7e5e79525883422))
* reuse clean inheritance parents across roots ([#156](https://github.com/ibiscite/ibis/issues/156)) ([863893b](https://github.com/ibiscite/ibis/commit/863893b0a9e579d0e9295b6bb61576cd9bf7eb26))
* scan author whitespace runs once ([#153](https://github.com/ibiscite/ibis/issues/153)) ([3526411](https://github.com/ibiscite/ibis/commit/35264112a827bafb5d60aeaedd7b9362c7796ce8))
* separate combobox focus from citation selection ([#181](https://github.com/ibiscite/ibis/issues/181)) ([f873888](https://github.com/ibiscite/ibis/commit/f873888a57bb8ef9441e7222fd46374696c1de6a))
* settle macOS smoke typing and synthetic editor windows ([#411](https://github.com/ibiscite/ibis/issues/411)) ([8217c3b](https://github.com/ibiscite/ibis/commit/8217c3ba7746dc88c56918907ef0959de6ad5142))
* show the macOS picker as a non-activating panel ([#214](https://github.com/ibiscite/ibis/issues/214)) ([b615c5e](https://github.com/ibiscite/ibis/commit/b615c5e14fe6ffb0bbe6aa7628076ab824135b08))
* size load budgets for practical large libraries ([#157](https://github.com/ibiscite/ibis/issues/157)) ([79eac38](https://github.com/ibiscite/ibis/commit/79eac3833642c39b4b062fa2c809b0858c8cb229))
* skip a Zotero item that cannot be exported instead of failing the library ([#412](https://github.com/ibiscite/ibis/issues/412)) ([e9d050a](https://github.com/ibiscite/ibis/commit/e9d050ad8a9bf8ddb3566fee905d761dea25003a))
* skip parsing an unchanged library when attaching its watcher ([#205](https://github.com/ibiscite/ibis/issues/205)) ([a98a988](https://github.com/ibiscite/ibis/commit/a98a98878ad8b7c4d589a9a0067a779fe8a2afee))
* synchronize release workspace lock versions ([#107](https://github.com/ibiscite/ibis/issues/107)) ([4f45649](https://github.com/ibiscite/ibis/commit/4f4564992a1778e219ffe88f218033e699892c2a)), closes [#106](https://github.com/ibiscite/ibis/issues/106)
* update the Zotero crate lock version during releases ([#277](https://github.com/ibiscite/ibis/issues/277)) ([89d123f](https://github.com/ibiscite/ibis/commit/89d123fd9ac4ab2d0029d2e862742431768c4763)), closes [#275](https://github.com/ibiscite/ibis/issues/275)
* use the bounded core loader for CLI libraries ([#168](https://github.com/ibiscite/ibis/issues/168)) ([deaaabc](https://github.com/ibiscite/ibis/commit/deaaabcc025c641a006f09762ad9c7433c4d9796))
* use X11 keyboard focus when toggling the picker ([#234](https://github.com/ibiscite/ibis/issues/234)) ([a736474](https://github.com/ibiscite/ibis/commit/a736474aec1724165e10b8ac7587e7abf590e6ec)), closes [#233](https://github.com/ibiscite/ibis/issues/233)
* validate Windows offline process creation identities ([#251](https://github.com/ibiscite/ibis/issues/251)) ([904b528](https://github.com/ibiscite/ibis/commit/904b528d96fc035ae886bda79c176833dd0fc3fc))
* warn on entry types broken by closing braces ([#177](https://github.com/ibiscite/ibis/issues/177)) ([bfbab66](https://github.com/ibiscite/ibis/commit/bfbab66bd5d2aba7680fbeadc7cce11c8deb1ae1))


### Performance

* index the desktop library once and open it off the main thread ([#74](https://github.com/ibiscite/ibis/issues/74)) ([6615a95](https://github.com/ibiscite/ibis/commit/6615a95596b946c692ceeb7d7e2f9a21cb325cd3))


### Documentation

* align issue management with standing authorization ([#212](https://github.com/ibiscite/ibis/issues/212)) ([da1d4c4](https://github.com/ibiscite/ibis/commit/da1d4c48b26f90984babf3b095825118c9f67def))
* clarify Windows setup and citation checks ([852dadc](https://github.com/ibiscite/ibis/commit/852dadcc81249b7a8c0c28a24f1608ea84e9f93a))
* count only work in progress toward the agent pull request limit ([#161](https://github.com/ibiscite/ibis/issues/161)) ([92ff794](https://github.com/ibiscite/ibis/commit/92ff794927d9a7cfb0e3e09b545bf058f30dc53d))
* decide signed, opt-in in-app updates with separate Nightly app ([#295](https://github.com/ibiscite/ibis/issues/295)) ([74332be](https://github.com/ibiscite/ibis/commit/74332be62f0be2bdcf0fd1e25d14b039851e7d93)), closes [#294](https://github.com/ibiscite/ibis/issues/294)
* describe the autonomous agent workflow and correct outdated pages ([#82](https://github.com/ibiscite/ibis/issues/82)) ([6694bb4](https://github.com/ibiscite/ibis/commit/6694bb41490718338eeb1ff63f7f54fe417e812c))
* explain how to use Ibis with Zotero through a kept-updated export ([#250](https://github.com/ibiscite/ibis/issues/250)) ([67c7870](https://github.com/ibiscite/ibis/commit/67c78708b758486eebbab683c6fac660f3cb31eb))
* explain the Windows WebView2 installation download ([48f4ffa](https://github.com/ibiscite/ibis/commit/48f4ffa8497ba40b7619dc7233301f667240d2dd))
* fix a code-tour test command and explain its terms ([#166](https://github.com/ibiscite/ibis/issues/166)) ([7157573](https://github.com/ibiscite/ibis/commit/7157573e64e7e3a7caf083e2eab06cc068ffa55e))
* let agents push unfinished work as drafts ([#148](https://github.com/ibiscite/ibis/issues/148)) ([eb7bd59](https://github.com/ibiscite/ibis/commit/eb7bd5937eebacbdce7cf47924bd4c7378fa3004))
* make 0.2 the shared preview with redesign, Zotero groups and safe updates ([#300](https://github.com/ibiscite/ibis/issues/300)) ([519b591](https://github.com/ibiscite/ibis/commit/519b591e887def493460587e06338ba1b9e98c89))
* make agent work reviewable, ordered and explainable ([#123](https://github.com/ibiscite/ibis/issues/123)) ([a31e4c0](https://github.com/ibiscite/ibis/commit/a31e4c0df14751cc1738b88d743be84740275d7a))
* name the default picker shortcut in the README ([#270](https://github.com/ibiscite/ibis/issues/270)) ([75a4ec1](https://github.com/ibiscite/ibis/commit/75a4ec1a43187598b1a36e2bb834454039bfcfe5))
* place researched features on the roadmap and list what is not planned ([#362](https://github.com/ibiscite/ibis/issues/362)) ([5c18bcf](https://github.com/ibiscite/ibis/commit/5c18bcf6dd96343c8eaaba46505a189cc4333445))
* record standing maintainer authorization for agents ([#187](https://github.com/ibiscite/ibis/issues/187)) ([ec369f0](https://github.com/ibiscite/ibis/commit/ec369f06d3125578d57e04efdb9b5811a52d8664))
* record the agreed picker design and window behaviour ([#242](https://github.com/ibiscite/ibis/issues/242)) ([cacca0e](https://github.com/ibiscite/ibis/commit/cacca0e7d9d3be4b8399282bb719d1b81397a64e))
* record the settings format and recovery contract ([#169](https://github.com/ibiscite/ibis/issues/169)) ([06efbca](https://github.com/ibiscite/ibis/commit/06efbca041fbba80e0d2ae12fa591d944cc686f9))
* refocus the roadmap on essentials ([#143](https://github.com/ibiscite/ibis/issues/143)) ([a03d81a](https://github.com/ibiscite/ibis/commit/a03d81ab6b7cc81f08f76e2fddb70f1d6b6fbac4))
* say the picker reads Zotero directly in the README status ([#280](https://github.com/ibiscite/ibis/issues/280)) ([0759da9](https://github.com/ibiscite/ibis/commit/0759da9cb98055e578d91939e02cde3bc3e3c334))
* set up development on Windows, macOS and Linux ([#125](https://github.com/ibiscite/ibis/issues/125)) ([fb5a53f](https://github.com/ibiscite/ibis/commit/fb5a53f6103d29ffa8d40b1044adb0f219b7019c))
* shorten the hand checks to a manual-only list ([#328](https://github.com/ibiscite/ibis/issues/328)) ([7440e19](https://github.com/ibiscite/ibis/commit/7440e1993fcb0330bcb00b6e074efa6c6b38e628))
* tighten agent rules and correct stale facts ([#318](https://github.com/ibiscite/ibis/issues/318)) ([ae1249d](https://github.com/ibiscite/ibis/commit/ae1249da702cb3bf8545db5f12be11090bd4299b))
* trace current picker code paths for maintainers ([#160](https://github.com/ibiscite/ibis/issues/160)) ([ec2c7e4](https://github.com/ibiscite/ibis/commit/ec2c7e45b2a0a58bafd372ba00623abeb344389d))

## Changelog

All notable changes to this project are documented in this file. Entries are generated from [Conventional Commits](https://www.conventionalcommits.org/) when a release is made, and versions follow [Semantic Versioning](https://semver.org/).
