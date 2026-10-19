// JXA through osascript/System Events. No browser automation or app test hooks.
ObjC.import('AppKit');
ObjC.import('ApplicationServices');
ObjC.import('CoreGraphics');
ObjC.import('CoreFoundation');

function run(argv) {
    const action = argv[0], appId = Number(argv[1]), editorId = Number(argv[2]), text = argv[3];
    const events = Application('System Events');
    const process = id => events.applicationProcesses.whose({unixId: id})()[0];
    const safe = (fn, fallback = '') => { try { return fn(); } catch (_) { return fallback; } };
    const numeric = value => typeof value === 'number' && Number.isFinite(value);
    const editorState = () => {
        const state = {bundle_verified: false, window_count: 0, document_metadata_count: 0,
            exact_document_url_matches: 0, owned_document_path_matches: 0, owned: false};
        const editor = $.NSRunningApplication.runningApplicationWithProcessIdentifier(editorId);
        if (!editor || ObjC.unwrap(editor.bundleIdentifier) !== 'com.apple.TextEdit') return state;
        state.bundle_verified = true;
        const windows = process(editorId).windows();
        if (windows.length > 20) throw Error('Synthetic editor window enumeration exceeded bound');
        state.window_count = windows.length;
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
    const elements = () => {
        const app = process(appId);
        if (!app || app.windows.length === 0) return [];
        const tree = app.windows[0].entireContents();
        if (tree.length > 4000) throw Error('Accessibility tree exceeded smoke traversal bound');
        return tree;
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
        const frames = accessibleWindows.map(window => {
            const position = window.position(), size = window.size();
            if (position.length !== 2 || size.length !== 2 ||
                !position.concat(size).every(numeric)) {
                throw Error('Accessible window bounds unavailable');
            }
            return {position, size};
        });
        // Hidden windows may remain in the AX tree. Match an on-screen CG
        // window to an AX picker frame; owner PID alone can match a tray item.
        const ownWindows = windows.filter(window => {
            if (!numeric(window.kCGWindowOwnerPID)) throw Error('Window ownership metadata unavailable');
            return Number(window.kCGWindowOwnerPID) === ownerId;
        });
        ownWindows.forEach(window => {
            const bounds = window.kCGWindowBounds || {};
            if (!numeric(window.kCGWindowAlpha) ||
                !['X', 'Y', 'Width', 'Height'].every(key => numeric(bounds[key]))) {
                throw Error('On-screen window bounds unavailable');
            }
        });
        return ownWindows.some(window => {
            const bounds = window.kCGWindowBounds || {};
            return Number(window.kCGWindowAlpha) > 0 &&
                Number(bounds.Width) > 0 && Number(bounds.Height) > 0 && frames.some(frame =>
                    frame.position.length === 2 && frame.size.length === 2 &&
                    Math.abs(frame.position[0] - Number(bounds.X)) <= 2 &&
                    Math.abs(frame.position[1] - Number(bounds.Y)) <= 2 &&
                    Math.abs(frame.size[0] - Number(bounds.Width)) <= 2 &&
                    Math.abs(frame.size[1] - Number(bounds.Height)) <= 2);
        });
    };
    let result;
    switch (action) {
    case 'probe':
        // AXIsProcessTrusted does not prompt or claim a grant. A hosted runner
        // without it is unavailable; no tccutil reset or permission bypass.
        if (!$.AXIsProcessTrusted()) throw Error('TCC Accessibility is not granted to osascript; native smoke unavailable');
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
    case 'editor-running': {
        const editors = $.NSRunningApplication.runningApplicationsWithBundleIdentifier('com.apple.TextEdit');
        result = false;
        for (let index = 0; index < Number(editors.count); index++) {
            if (Number(editors.objectAtIndex(index).processIdentifier) === editorId) result = true;
        }
        break;
    }
    case 'snapshot': {
        const app = process(appId);
        const nodes = [];
        elements().forEach(item => {
            // WebKit maps labels to AXTitle or AXDescription depending on role.
            // Preserve each exposed name rather than guess which one it chose.
            const names = new Set([safe(() => item.title()), safe(() => item.description()), safe(() => item.name())]);
            names.forEach(name => nodes.push({
                role: safe(() => item.role()), name,
                value: String(safe(() => item.value())),
                focused: safe(() => item.attributes.byName('AXFocused').value(), false) === true,
            }));
        });
        const visible = onScreen(appId, app ? app.windows() : []);
        // A nonactivating panel intentionally leaves the editor application
        // frontmost while accepting keyboard input in its own search field.
        // The AX keyboard focus is the relevant foreground proof here.
        result = {nodes, visible,
            foreground: visible && nodes.some(node => node.focused && node.name === 'Search references')};
        break;
    }
    case 'key': {
        const keys = text.toLowerCase().split('+');
        const last = keys.pop();
        const mods = {ctrl: 'control down', shift: 'shift down', super: 'command down', alt: 'option down'};
        const using = keys.map(key => { if (!mods[key]) throw Error('Unsupported key'); return mods[key]; });
        const codes = {escape: 53, return: 36, space: 49};
        if (Object.hasOwn(codes, last)) events.keyCode(codes[last], {using});
        else if (/^[a-z]$/.test(last)) events.keystroke(last, {using});
        else throw Error('Unsupported key');
        result = true;
        break;
    }
    case 'text': events.keystroke(text); result = true; break;
    case 'click': {
        const aliases = text === 'Reload' ? ['Reload', 'Reload library'] :
            text === 'Choose library…' ? ['Choose library…', 'Choose a library'] : [text];
        const buttons = elements().filter(item => safe(() => item.role()) === 'AXButton');
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
        // Watcher notices can change the WebKit tree between role and label
        // reads. Only absence is retried by Python's bounded native wait.
        // An AXPress failure propagates immediately and is never retried.
        if (matches.length === 1) matches[0].actions.byName('AXPress').perform();
        result = {pressed: matches.length === 1, button_count: buttons.length,
            exact_label_matches: matches.length, successful_attribute_reads: successfulReads,
            attribute_types: [...types].sort(), known_action: ['Reload', 'Choose library…'].includes(text)};
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
