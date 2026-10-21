// JXA through osascript/System Events. No browser automation or app test hooks.
// Diagnostic wall-clock milliseconds only; subprocess setup before JavaScript
// and result serialization are outside this origin. Clamp clock corrections.
const helperStarted = Date.now();
ObjC.import('AppKit');
ObjC.import('ApplicationServices');
ObjC.import('CoreGraphics');
ObjC.import('CoreFoundation');

// Global HID events follow the real key window, including a nonactivating
// panel whose application intentionally is not frontmost. The callback keeps
// event plans testable without claiming a native post happened on other OSes.
function sendPhysicalChord(chord, post) {
    const modifiers = {ctrl: [59, 1 << 18], shift: [56, 1 << 17],
        super: [55, 1 << 20], alt: [58, 1 << 19]};
    const codes = {a: 0, g: 5, r: 15, v: 9, escape: 53, return: 36, space: 49};
    const keys = chord.toLowerCase().split('+');
    const last = keys.pop();
    if (!Object.hasOwn(codes, last) || keys.some(key => !Object.hasOwn(modifiers, key)) || new Set(keys).size !== keys.length) {
        throw Error('Unsupported native HID chord');
    }
    const held = [];
    let flags = 0, baseHeld = false, error;
    try {
        keys.forEach(key => {
            const modifier = modifiers[key];
            flags |= modifier[1];
            held.push(modifier);
            post(modifier[0], true, flags);
        });
        baseHeld = true;
        post(codes[last], true, flags);
        post(codes[last], false, flags);
        baseHeld = false;
    } catch (failure) { error = failure; }
    // Attempt every release even if a later event could not be created.
    if (baseHeld) {
        try { post(codes[last], false, flags); } catch (failure) { error ||= failure; }
    }
    held.reverse().forEach(([code, mask]) => {
        flags &= ~mask;
        try { post(code, false, flags); } catch (failure) { error ||= failure; }
    });
    if (error) throw error;
}

function sendUnicodeText(text, post) {
    for (const character of text) {
        let error;
        try { post(0, true, 0, character); } catch (failure) { error = failure; }
        try { post(0, false, 0, character); } catch (failure) { error ||= failure; }
        if (error) throw error;
    }
}

function preflightPostAccess() {
    // Older BridgeSupport metadata omits this public macOS 10.15+ symbol.
    // Its SDK declaration is bool CGPreflightPostEventAccess(void).
    // https://developer.apple.com/documentation/coregraphics/cgpreflightposteventaccess()?language=objc
    if (typeof $.CGPreflightPostEventAccess !== 'function') {
        try { ObjC.bindFunction('CGPreflightPostEventAccess', ['bool', []]); }
        catch (_) { throw Error('Native event-posting preflight unavailable'); }
    }
    if (typeof $.CGPreflightPostEventAccess !== 'function') throw Error('Native event-posting preflight unavailable');
    const granted = $.CGPreflightPostEventAccess();
    if (typeof granted !== 'boolean') throw Error('Native event-posting preflight unavailable');
    return granted;
}

function readEditorFullscreen(read) {
    let state;
    // AXFullScreen can briefly be unreadable while a Space transition settles.
    // Unknown is explicit; it must never count as the requested false state.
    try { state = read(); }
    catch (_) { return {available: false, fullscreen: null}; }
    if (typeof state !== 'boolean') throw Error('Editor fullscreen accessibility state unavailable');
    return {available: true, fullscreen: state};
}

function readEditorWindowState(read) {
    // Right after a Space transition, System Events can briefly drop the
    // window it just listed from its index. Only that stale reference is an
    // unknown state to wait out; permission and window-count errors still fail.
    try { return read(); }
    catch (error) {
        const details = String(error);
        if (!/Invalid index\. \(-1719\)/.test(details) || /assistive access/i.test(details)) throw error;
        return {available: false, fullscreen: null, visible: false};
    }
}

function run(argv) {
    const action = argv[0], appId = Number(argv[1]), editorId = Number(argv[2]), text = argv[3];
    const events = Application('System Events');
    const process = id => events.applicationProcesses.whose({unixId: id})()[0];
    const safe = (fn, fallback = '') => { try { return fn(); } catch (_) { return fallback; } };
    const numeric = value => typeof value === 'number' && Number.isFinite(value);
    const postEvent = () => {
        if (!preflightPostAccess()) throw Error('TCC event posting is not granted; native smoke unavailable');
        const source = $.CGEventSourceCreate($.kCGEventSourceStateHIDSystemState);
        if (!source) throw Error('Native HID event source unavailable');
        let unicodeBridgeReady = false;
        return (code, down, flags, character) => {
            const event = $.CGEventCreateKeyboardEvent(source, code, down);
            if (!event) throw Error('Native HID keyboard event unavailable');
            $.CGEventSetFlags(event, flags);
            if (character !== undefined) {
                if (!unicodeBridgeReady) {
                    // NSData.bytes is void*, while BridgeSupport insists on a
                    // UniChar* Ref. Both are pointer arguments in the public
                    // C ABI; UniCharCount is unsigned long on 64-bit macOS.
                    // https://developer.apple.com/documentation/coregraphics/cgevent/keyboardsetunicodestring(stringlength:unicodestring:)?language=objc
                    // https://developer.apple.com/library/archive/documentation/Carbon/Conceptual/Carbon64BitGuide/PortingTo64Bit/PortingTo64Bit.html
                    try { ObjC.bindFunction('CGEventKeyboardSetUnicodeString', ['void', ['void *', 'unsigned long', 'void *']]); }
                    catch (_) { throw Error('Native HID Unicode bridge unavailable'); }
                    unicodeBridgeReady = true;
                }
                // NSString/NSData retain the UTF-16 storage until posting.
                // Each codepoint uses at most two units, retaining surrogates.
                const data = $(character).dataUsingEncoding($.NSUTF16LittleEndianStringEncoding);
                if (!data || Number(data.length) !== character.length * 2) throw Error('Native HID Unicode storage unavailable');
                $.CGEventKeyboardSetUnicodeString(event, character.length, data.bytes);
                $.CGEventPost($.kCGHIDEventTap, event);
            } else $.CGEventPost($.kCGHIDEventTap, event);
        };
    };
    const documentIdentity = () => {
        const expected = ObjC.unwrap($.NSURL.fileURLWithPath(text).absoluteString);
        const localPath = value => {
            if (typeof value !== 'string' || !value.startsWith('file:')) return null;
            const url = $.NSURL.URLWithString(value);
            if (!url || !url.isFileURL) return null;
            const host = ObjC.unwrap(url.host) || '';
            if (host !== '' && host !== 'localhost') return null;
            return ObjC.unwrap(url.URLByResolvingSymlinksInPath.path);
        };
        const expectedPath = localPath(expected);
        if (!expectedPath) throw Error('Synthetic editor document identity unavailable');
        return {expected, expectedPath, localPath};
    };
    const editorState = () => {
        const state = {bundle_verified: false, window_count: 0, document_metadata_count: 0,
            exact_document_url_matches: 0, owned_document_path_matches: 0, owned: false};
        const editor = $.NSRunningApplication.runningApplicationWithProcessIdentifier(editorId);
        if (!editor || ObjC.unwrap(editor.bundleIdentifier) !== 'com.apple.TextEdit') return state;
        state.bundle_verified = true;
        const windows = process(editorId).windows();
        if (windows.length > 20) throw Error('Synthetic editor window enumeration exceeded bound');
        state.window_count = windows.length;
        const {expected, expectedPath, localPath} = documentIdentity();
        windows.forEach(window => {
            const document = safe(() => window.attributes.byName('AXDocument').value());
            if (typeof document !== 'string' || document.length === 0) return;
            state.document_metadata_count++;
            if (document === expected) state.exact_document_url_matches++;
            if (localPath(document) === expectedPath) state.owned_document_path_matches++;
        });
        // A fresh process could restore old windows. Never terminate an
        // instance containing any window other than our sole owned document.
        state.owned = windows.length === 1 && state.owned_document_path_matches === 1;
        return state;
    };
    const frame = window => {
        const position = window.position(), size = window.size();
        if (position.length !== 2 || size.length !== 2 || !position.concat(size).every(numeric)) {
            throw Error('Accessible window bounds unavailable');
        }
        return {position, size};
    };
    const onScreen = (ownerId, accessibleWindows) => {
        const windowRefs = $.CGWindowListCopyWindowInfo(
            $.kCGWindowListOptionOnScreenOnly | $.kCGWindowListExcludeDesktopElements,
            $.kCGNullWindowID);
        if (!windowRefs) throw Error('Native on-screen window metadata unavailable');
        // Recent JXA versions leave the top-level CFArray opaque to deepUnwrap.
        // Read its bounded native elements, then bridge each CFDictionary.
        const count = Number($.CFArrayGetCount(windowRefs));
        if (!Number.isInteger(count) || count < 0 || count > 10000) throw Error('Native window enumeration exceeded bound');
        const windows = [];
        for (let index = 0; index < count; index++) {
            const reference = $.CFArrayGetValueAtIndex(windowRefs, index);
            if (!reference) throw Error('Native on-screen window entry unavailable');
            const window = ObjC.deepUnwrap(ObjC.castRefToObject(reference));
            if (!window || typeof window !== 'object' || Array.isArray(window)) {
                throw Error('Native on-screen window entry is not a dictionary');
            }
            windows.push(window);
        }
        const frames = accessibleWindows.map(frame);
        const owned = windows.filter(window => {
            if (!numeric(window.kCGWindowOwnerPID)) throw Error('Window ownership metadata unavailable');
            return Number(window.kCGWindowOwnerPID) === ownerId;
        });
        owned.forEach(window => {
            const bounds = window.kCGWindowBounds || {};
            if (!numeric(window.kCGWindowAlpha) ||
                !['X', 'Y', 'Width', 'Height'].every(key => numeric(bounds[key]))) {
                throw Error('On-screen window bounds unavailable');
            }
        });
        return owned.some(window => {
            const bounds = window.kCGWindowBounds;
            return Number(window.kCGWindowAlpha) > 0 && Number(bounds.Width) > 0 && Number(bounds.Height) > 0 &&
                frames.some(candidate =>
                    Math.abs(candidate.position[0] - bounds.X) <= 2 &&
                    Math.abs(candidate.position[1] - bounds.Y) <= 2 &&
                    Math.abs(candidate.size[0] - bounds.Width) <= 2 &&
                    Math.abs(candidate.size[1] - bounds.Height) <= 2);
        });
    };
    const contents = element => {
        const tree = element.entireContents();
        if (tree.length > 4000) throw Error('Accessibility tree exceeded smoke traversal bound');
        return [element].concat(tree);
    };
    const labelled = (element, label) => [safe(() => element.title()), safe(() => element.description()),
        safe(() => element.name()), safe(() => element.value())].includes(label);
    const focused = element => safe(() => element.attributes.byName('AXFocused').value(), false) === true;
    const pickerWindows = () => {
        const app = process(appId);
        return app ? app.windows().filter(window => labelled(window, 'Ibis')) : [];
    };
    const editorWindow = () => {
        const editor = process(editorId);
        const windows = editor ? editor.windows() : [];
        if (windows.length !== 1) throw Error(`A single synthetic editor window is required (found ${windows.length})`);
        return windows[0];
    };
    const fullscreen = window => {
        let state;
        try { state = window.attributes.byName('AXFullScreen').value(); }
        catch (_) { throw Error('Editor fullscreen accessibility state unavailable'); }
        if (typeof state !== 'boolean') throw Error('Editor fullscreen accessibility state unavailable');
        return state;
    };
    const editorWindowState = () => readEditorWindowState(() => {
        const window = editorWindow();
        // Use the same owned window for AX state and CG visibility. Unknown
        // AX state never counts as the requested non-fullscreen state.
        const state = readEditorFullscreen(() => window.attributes.byName('AXFullScreen').value());
        return {...state, visible: onScreen(editorId, [window])};
    });
    const chooser = () => {
        const app = process(appId);
        if (!app) throw Error('Native picker process unavailable');
        const candidates = [];
        app.windows().forEach(window => contents(window).forEach(element => {
            const role = safe(() => element.role()), subrole = safe(() => element.subrole());
            const titledWindow = role === 'AXWindow' && labelled(element, 'Choose a BibTeX or BibLaTeX library');
            if (role !== 'AXSheet' && role !== 'AXDialog' && subrole !== 'AXDialog' && !titledWindow) return;
            const nodes = contents(element);
            if (nodes.some(node => labelled(node, 'Choose a BibTeX or BibLaTeX library'))) {
                candidates.push({element, nodes});
            }
        }));
        if (candidates.length > 10) throw Error('Native chooser scope exceeded smoke traversal bound');
        return candidates.find(candidate => onScreen(appId, [candidate.element]));
    };
    const elements = () => {
        const app = process(appId);
        if (!app || app.windows.length === 0) return [];
        const tree = app.windows[0].entireContents();
        if (tree.length > 4000) throw Error('Accessibility tree exceeded smoke traversal bound');
        return tree;
    };
    let result;
    switch (action) {
    case 'probe':
        // AXIsProcessTrusted does not prompt or claim a grant. A hosted runner
        // without it is unavailable; no tccutil reset or permission bypass.
        if (!$.AXIsProcessTrusted()) throw Error('TCC Accessibility is not granted to osascript; native smoke unavailable');
        if (!preflightPostAccess()) throw Error('TCC event posting is not granted; native smoke unavailable');
        // Query the real scripting object. The obsolete UI-elements-enabled
        // property is not a valid JXA class on the macOS 15 hosted image.
        events.applicationProcesses.length; // Also checks Automation consent.
        result = true;
        break;
    case 'editor-preflight':
        if (Number($.NSRunningApplication.runningApplicationsWithBundleIdentifier('com.apple.TextEdit').count) !== 0) {
            throw Error('Use a disposable desktop without an existing TextEdit process');
        }
        result = true;
        break;
    case 'editor': {
        const editors = $.NSRunningApplication.runningApplicationsWithBundleIdentifier('com.apple.TextEdit');
        if (Number(editors.count) === 0) { result = 0; break; }
        if (Number(editors.count) !== 1) throw Error('Expected exactly one newly launched synthetic editor');
        result = Number(editors.objectAtIndex(0).processIdentifier);
        break;
    }
    case 'editor-owned': result = editorState().owned; break;
    case 'editor-state': result = editorState(); break;
    case 'close-extra-editor-windows': {
        // Only the verified fresh instance is touched, and only windows that
        // are not the owned document, such as ones restored from saved state.
        if (!editorState().bundle_verified) throw Error('Synthetic editor identity unavailable');
        const {expectedPath, localPath} = documentIdentity();
        const windows = process(editorId).windows();
        if (windows.length > 20) throw Error('Synthetic editor window enumeration exceeded bound');
        let closed = 0;
        windows.forEach(window => {
            const document = safe(() => window.attributes.byName('AXDocument').value());
            if (localPath(document) === expectedPath) return;
            const close = safe(() => window.buttons.whose({subrole: 'AXCloseButton'})()[0], null);
            if (!close) throw Error('Leftover synthetic editor window cannot be closed');
            close.actions.byName('AXPress').perform();
            closed++;
        });
        result = closed;
        break;
    }
    case 'editor-running': {
        const editors = $.NSRunningApplication.runningApplicationsWithBundleIdentifier('com.apple.TextEdit');
        result = false;
        for (let index = 0; index < Number(editors.count); index++) {
            if (Number(editors.objectAtIndex(index).processIdentifier) === editorId) result = true;
        }
        break;
    }
    case 'snapshot': {
        const treeStarted = Date.now();
        const elapsed = start => Math.max(0, Date.now() - start);
        const probe_phases = {bootstrap_ms: Math.max(0, treeStarted - helperStarted)};
        const app = process(appId);
        const nodes = [];
        const items = elements();
        probe_phases.tree_ms = elapsed(treeStarted);
        const propertiesStarted = Date.now();
        items.forEach(item => {
            // WebKit maps labels to AXTitle or AXDescription depending on role.
            // Preserve each exposed name rather than guess which one it chose.
            const names = new Set([safe(() => item.title()), safe(() => item.description()), safe(() => item.name())]);
            // Each alias describes this same element, so observe its state once.
            // Keep fresh reads for every element and every snapshot invocation.
            const role = safe(() => item.role());
            const value = String(safe(() => item.value()));
            const focused = safe(() => item.attributes.byName('AXFocused').value(), false) === true;
            names.forEach(name => nodes.push({role, name, value, focused}));
        });
        probe_phases.properties_ms = elapsed(propertiesStarted);
        const visibilityStarted = Date.now();
        // Hidden windows may remain in the AX tree. Match an on-screen CG
        // window to an AX picker frame; owner PID alone can match a tray item.
        // The Copied notice (#496) is a second window that is never the picker.
        const visible = onScreen(appId, app ? app.windows().filter(window => !labelled(window, 'Ibis notice')) : []);
        probe_phases.visibility_ms = elapsed(visibilityStarted);
        probe_phases.helper_ms = elapsed(helperStarted);
        probe_phases.elements = items.length;
        // A nonactivating panel intentionally leaves the editor application
        // frontmost while accepting keyboard input in its own search field.
        // The AX keyboard focus is the relevant foreground proof here.
        result = {nodes, visible, probe_phases,
            foreground: visible && nodes.some(node => node.focused && node.name === 'Search references')};
        break;
    }
    case 'editor-window-state': {
        result = editorWindowState();
        break;
    }
    case 'set-editor-fullscreen': {
        if (!['true', 'false'].includes(text)) throw Error('Unsupported fullscreen request');
        const desired = text === 'true';
        if (fullscreen(editorWindow()) !== desired) {
            try { editorWindow().attributes.byName('AXFullScreen').value = desired; }
            catch (_) { throw Error('Editor fullscreen accessibility control unavailable'); }
        }
        const deadline = Date.now() + 20000;
        while (true) {
            if (Date.now() >= deadline) throw Error('Editor fullscreen transition did not complete on screen');
            const state = editorWindowState();
            if (state.available === true && state.fullscreen === desired && state.visible === true) {
                // Return this observation instead of querying again after the
                // wait, when a Space transition could change the AX tree.
                result = {fullscreen: state.fullscreen, visible: state.visible};
                break;
            }
            $.NSThread.sleepForTimeInterval(0.1);
        }
        break;
    }
    case 'picker-key': {
        // AppKit answers a window's AXFocused from its key-window status, so
        // this is keyboard ownership. The search field's AXFocused is only the
        // WebView's DOM focus, which stays set after the panel resigns key.
        const keyWindows = pickerWindows().filter(focused);
        result = keyWindows.length > 0 && onScreen(appId, keyWindows);
        break;
    }
    case 'chooser-state': {
        const dialog = chooser();
        result = {visible: !!dialog, focused: !!dialog && dialog.nodes.some(focused),
            picker_visible: onScreen(appId, pickerWindows())};
        break;
    }
    case 'cancel-chooser': {
        const dialog = chooser();
        if (!dialog) throw Error('The named native library chooser is not on screen');
        const cancel = dialog.nodes.find(node => safe(() => node.role()) === 'AXButton' && labelled(node, 'Cancel'));
        if (!cancel) throw Error('Native library chooser Cancel control unavailable');
        cancel.actions.byName('AXPress').perform();
        result = true;
        break;
    }
    case 'key': {
        sendPhysicalChord(text, postEvent());
        result = true;
        break;
    }
    case 'text': sendUnicodeText(text, postEvent()); result = true; break;
    case 'click': {
        const aliases = text === 'Reload' ? ['Reload', 'Reload library'] :
            text === 'Choose library…' ? ['Choose library…', 'Choose a library'] :
            text === 'More' ? ['More', 'More actions (⌘K)'] : [text];
        // WebKit exposes menu buttons and their items with their own roles.
        const pressable = new Set(['AXButton', 'AXPopUpButton', 'AXMenuButton', 'AXMenuItem']);
        const toggle = item => safe(() => item.role()) === 'AXCheckBox' && safe(() => item.subrole()) === 'AXToggle';
        const buttons = elements().filter(item => pressable.has(safe(() => item.role())) || toggle(item));
        let successfulReads = 0;
        const types = new Set();
        const matches = buttons.filter(item => {
            const read = fn => {
                let value;
                try { value = fn(); successfulReads++; } catch (_) { /* Count unavailable reads without raw errors. */ }
                types.add(typeof value);
                return value;
            };
            const labels = [read(() => item.description()), read(() => item.name()),
                read(() => item.title()), read(() => item.value()),
                ...['AXTitle', 'AXDescription', 'AXHelp', 'AXValue'].map(attribute =>
                    read(() => item.attributes.byName(attribute).value()))];
            return labels.some(label => aliases.includes(label));
        });
        if (matches.length > 1) throw Error('Named native button is ambiguous');
        // Observe control roles before activation: Quit or a menu transition
        // can invalidate the AX element. These facts describe the control,
        // not the resulting checkbox value or application state.
        result = {pressed: matches.length === 1, button_count: buttons.length,
            exact_label_matches: matches.length, successful_attribute_reads: successfulReads,
            attribute_types: [...types].sort(), known_action: ['Reload', 'Choose library…', 'More'].includes(text),
            button_role: matches.length === 1 && safe(() => matches[0].role()) === 'AXButton',
            toggle_role: matches.length === 1 && toggle(matches[0])};
        // Watcher notices can change the WebKit tree between role and label
        // reads. Only absence is retried by Python's bounded native wait.
        // An AXPress failure propagates immediately and is never retried.
        if (matches.length === 1) matches[0].actions.byName('AXPress').perform();
        break;
    }
    case 'focus-editor': process(editorId).frontmost = true; result = true; break;
    case 'editor-focused': result = process(editorId).frontmost(); break;
    case 'editor-focus-state': {
        const editor = $.NSRunningApplication.runningApplicationWithProcessIdentifier(editorId);
        const frontmost = $.NSWorkspace.sharedWorkspace.frontmostApplication;
        if (!editor || !frontmost) throw Error('Native editor focus metadata unavailable');
        const activeId = Number(frontmost.processIdentifier);
        if (!Number.isInteger(activeId) || activeId <= 0) throw Error('Native foreground ownership unavailable');
        const editorFrontmost = process(editorId).frontmost(), editorActive = editor.isActive;
        if (typeof editorFrontmost !== 'boolean' || typeof editorActive !== 'boolean') throw Error('Native editor focus flags unavailable');
        result = {system_events_editor_frontmost: editorFrontmost,
            workspace_editor_frontmost: activeId === editorId, workspace_ibis_frontmost: activeId === appId,
            workspace_other_frontmost: activeId !== editorId && activeId !== appId,
            editor_running_application_active: editorActive,
            editor_window_visible: onScreen(editorId, process(editorId).windows())};
        break;
    }
    case 'clipboard': result = ObjC.unwrap($.NSPasteboard.generalPasteboard.stringForType($.NSPasteboardTypeString)) || ''; break;
    case 'write-clipboard':
        $.NSPasteboard.generalPasteboard.clearContents;
        if (!$.NSPasteboard.generalPasteboard.setStringForType(text, $.NSPasteboardTypeString)) throw Error('Synthetic clipboard write failed');
        result = true;
        break;
    case 'memory': {
        const rows = Application.currentApplication();
        rows.includeStandardAdditions = true;
        const all = rows.doShellScript('/bin/ps -axo pid=,ppid=,rss=').split('\r').join('\n').trim().split('\n').map(line => line.trim().split(/\s+/).map(Number));
        const ids = new Set([appId]);
        let added;
        do {
            added = false;
            all.forEach(([pid, parent]) => { if (ids.has(parent) && !ids.has(pid)) { ids.add(pid); added = true; } });
        } while (added);
        result = all.filter(([pid]) => ids.has(pid)).reduce((total, row) => total + row[2] * 1024, 0);
        break;
    }
    default: throw Error('Unknown smoke action');
    }
    return JSON.stringify(result);
}
