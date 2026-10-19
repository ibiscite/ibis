// Ordinary Accessibility UI only. Never write preferences or request TCC grants.
ObjC.import('AppKit');
ObjC.import('ApplicationServices');
ObjC.import('CoreGraphics');
ObjC.import('CoreFoundation');

// Inspect only the owned picker's window metadata, never its WebView tree.
function pickerAction(action, request, stage = () => {}) {
    stage('identity');
    const token = action === 'picker-owner' ? request : request.owner;
    if (!token || !Number.isInteger(token.pid) || token.pid <= 0 ||
        typeof token.executable !== 'string' || !token.executable.startsWith('/')) {
        throw Error('Owned picker identity unavailable');
    }
    // Objective-C nil is a truthy JXA wrapper. Only the pre-token owner
    // registration poll may wait for absent public registration metadata.
    const missing = value => !value || (typeof value.isNil === 'function' && value.isNil());
    const unavailable = () => {
        if (action === 'picker-owner') return null;
        throw Error('Owned picker identity changed');
    };
    stage('registration');
    const app = $.NSRunningApplication.runningApplicationWithProcessIdentifier(token.pid);
    if (missing(app)) return unavailable();
    if (app.isTerminated !== false || Number(app.processIdentifier) !== token.pid) {
        throw Error('Owned picker identity changed');
    }
    // launchDate is explicitly unavailable for direct, non-LaunchServices
    // launches (NSRunningApplication.h). Python pins the unreaped Popen child
    // around every call instead.
    const executable = app.executableURL;
    if (missing(executable)) return unavailable();
    if (ObjC.unwrap(executable.path) !== token.executable) throw Error('Owned picker identity changed');
    if (action === 'picker-owner') return {pid: token.pid, executable: token.executable};
    stage('process');
    const processes = Application('System Events').applicationProcesses.whose({unixId: token.pid})();
    if (processes.length !== 1) throw Error('Owned picker identity unavailable');
    stage('windows');
    const windows = processes[0].windows();
    if (windows.length > 10) throw Error('Picker window enumeration exceeded bound');
    const matches = windows.filter(window => window.role() === 'AXWindow' && window.name() === 'Ibis');
    if (matches.length !== 1) throw Error('Picker window identity is ambiguous');
    stage('geometry');
    const window = matches[0], position = window.position(), size = window.size();
    const pair = value => Array.isArray(value) && value.length === 2 && value.every(Number.isFinite);
    if (!pair(position) || !pair(size) || size.some(value => value <= 0)) throw Error('Picker bounds unavailable');
    stage('ownership');
    const refs = $.CGWindowListCopyWindowInfo(
        $.kCGWindowListOptionOnScreenOnly | $.kCGWindowListExcludeDesktopElements, $.kCGNullWindowID);
    if (!refs) throw Error('Picker on-screen ownership unavailable');
    const count = Number($.CFArrayGetCount(refs)), owned = [];
    if (!Number.isInteger(count) || count < 0 || count > 10000) throw Error('Native window enumeration exceeded bound');
    for (let index = 0; index < count; index++) {
        const ref = $.CFArrayGetValueAtIndex(refs, index);
        if (!ref) throw Error('Native window ownership unavailable');
        const info = ObjC.deepUnwrap(ObjC.castRefToObject(ref));
        if (info.kCGWindowOwnerPID !== token.pid) continue;
        const bounds = info.kCGWindowBounds;
        if (!bounds || ![bounds.X, bounds.Y, bounds.Width, bounds.Height, info.kCGWindowAlpha].every(Number.isFinite)) {
            throw Error('Owned native window bounds unavailable');
        }
        if (info.kCGWindowAlpha > 0 && Math.abs(bounds.X - position[0]) <= 2 && Math.abs(bounds.Y - position[1]) <= 2 &&
            Math.abs(bounds.Width - size[0]) <= 2 && Math.abs(bounds.Height - size[1]) <= 2) owned.push(info);
    }
    if (owned.length !== 1 || !Number.isInteger(owned[0].kCGWindowNumber) || owned[0].kCGWindowNumber <= 0) {
        throw Error('Picker on-screen ownership is ambiguous or unavailable');
    }
    const screen = $.NSScreen.mainScreen.frame.size;
    const frame = {window: owned[0].kCGWindowNumber, position, size, screen: [Number(screen.width), Number(screen.height)]};
    if (action === 'picker-frame') return frame;
    stage('resize');
    if (request.window !== frame.window) throw Error('Owned picker window identity changed');
    const target = request.size;
    if (!pair(target) || !target.every(Number.isInteger) || target[0] < 360 || target[1] < 320 ||
        target[0] > 1280 || target[1] > 900 || !pair(frame.screen) || position.some(value => value < 0) ||
        position[0] + target[0] > frame.screen[0] || position[1] + target[1] > frame.screen[1]) {
        throw Error('Requested picker size is outside the capture bounds');
    }
    // One native mutation. The Python observer verifies fresh readback before
    // any resized capture and restores the original size even after failure.
    window.size = target;
    return true;
}

function run(argv) {
    const action = argv[0], identity = argv[1], argument = argv[2];
    if (['picker-owner', 'picker-frame', 'picker-size'].includes(action)) {
        let stage = 'identity';
        try {
            return JSON.stringify(pickerAction(action, JSON.parse(argument), value => { stage = value; }));
        } catch (error) {
            // Never serialize native exception text, paths or request payloads.
            const reasons = {
                'Owned picker identity unavailable': 'invalid-identity',
                'Owned picker identity changed': 'identity-changed',
                'Owned picker window identity changed': 'identity-changed',
                'Picker window identity is ambiguous': 'ambiguous-window',
                'Picker on-screen ownership is ambiguous or unavailable': 'ambiguous-window',
                'Requested picker size is outside the capture bounds': 'invalid-size'
            };
            const reason = Object.prototype.hasOwnProperty.call(reasons, error.message) ? reasons[error.message] : 'native-call-failed';
            return JSON.stringify({nativeAppearanceFailure: {action, stage, reason}});
        }
    }
    const bundle = 'com.apple.systempreferences';
    const events = Application('System Events');
    const safe = fn => { try { return fn(); } catch (_) { return undefined; } };
    const running = () => $.NSRunningApplication.runningApplicationsWithBundleIdentifier(bundle);
    const owner = () => {
        const apps = running();
        if (Number(apps.count) === 0) return '';
        if (Number(apps.count) !== 1) throw Error('System Settings process identity is ambiguous');
        const app = apps.objectAtIndex(0);
        return JSON.stringify({pid: Number(app.processIdentifier), started: Number(app.launchDate.timeIntervalSince1970)});
    };
    const owned = () => {
        const token = JSON.parse(identity);
        if (!Number.isInteger(token.pid) || token.pid <= 0 || !Number.isFinite(token.started) || token.started <= 0) {
            throw Error('Owned System Settings identity unavailable');
        }
        const app = $.NSRunningApplication.runningApplicationWithProcessIdentifier(token.pid);
        if (!app || ObjC.unwrap(app.bundleIdentifier) !== bundle || Number(app.launchDate.timeIntervalSince1970) !== token.started) {
            throw Error('Owned System Settings identity changed');
        }
        return {app, process: events.applicationProcesses.whose({unixId: token.pid})()[0]};
    };
    const controls = () => {
        const windows = owned().process.windows();
        if (windows.length > 10) throw Error('System Settings window traversal exceeded bound');
        const labels = {contrast: 'Increase contrast', transparency: 'Reduce transparency'};
        const result = {};
        let count = 0;
        // Check every fresh owned array: unchanged counts or positions cannot
        // establish uniqueness. One shared pass reads each node's role once,
        // then reads labels only for checkbox nodes, unlike the original
        // separate full-tree pass for each preference.
        for (let window = 0; window < windows.length; window++) {
            const nodes = windows[window].entireContents();
            count += nodes.length;
            if (count > 6000) throw Error('System Settings control traversal exceeded bound');
            nodes.forEach(node => {
                if (safe(() => node.role()) !== 'AXCheckBox') return;
                const names = [safe(() => node.title()), safe(() => node.description()), safe(() => node.name())];
                for (const key of Object.keys(labels)) {
                    if (!names.includes(labels[key])) continue;
                    if (result[key]) throw Error('Exact accessibility preference control is ambiguous');
                    result[key] = node;
                }
            });
        }
        return result;
    };
    const state = found => {
        const result = {};
        for (const key of ['contrast', 'transparency']) {
            if (!found[key]) throw Error('Exact accessibility preference control unavailable');
            const value = found[key].value();
            if (typeof value !== 'boolean' && value !== 0 && value !== 1) throw Error('Native preference value is not boolean');
            result[key] = value === true || value === 1;
        }
        return result;
    };
    let result;
    switch (action) {
    case 'probe':
        if (!$.AXIsProcessTrusted()) throw Error('TCC Accessibility is not granted; native appearance unavailable');
        events.applicationProcesses.length;
        if (Number(running().count) !== 0) throw Error('Use a disposable desktop without an existing System Settings process');
        result = true;
        break;
    case 'owner': result = owner(); break;
    case 'controls-ready': {
        const found = controls();
        result = {ready: Boolean(found.contrast && found.transparency)};
        break;
    }
    case 'read': {
        const found = controls();
        result = state(found);
        break;
    }
    case 'set': {
        const requested = JSON.parse(argument);
        if (!['contrast', 'transparency'].includes(requested.key) || typeof requested.value !== 'boolean') {
            throw Error('Unsupported accessibility preference request');
        }
        const found = controls();
        const changed = state(found)[requested.key] !== requested.value;
        if (changed) found[requested.key].actions.byName('AXPress').perform();
        result = {changed};
        break;
    }
    case 'close':
        result = {requested: owned().app.terminate() === true};
        break;
    case 'closed': {
        const token = JSON.parse(identity);
        const app = $.NSRunningApplication.runningApplicationWithProcessIdentifier(token.pid);
        result = !app || app.isTerminated === true || ObjC.unwrap(app.bundleIdentifier) !== bundle ||
            Number(app.launchDate.timeIntervalSince1970) !== token.started;
        break;
    }
    case 'force-close':
        if (JSON.parse(argument).restoration_verified !== true) throw Error('Force quit requires verified preference restoration');
        if (!owned().app.forceTerminate()) throw Error('Owned System Settings force quit failed');
        result = true;
        break;
    case 'backdrop': {
        const app = $.NSApplication.sharedApplication;
        app.setActivationPolicy($.NSApplicationActivationPolicyAccessory);
        const rect = $.NSScreen.mainScreen.frame;
        const window = $.NSWindow.alloc.initWithContentRectStyleMaskBackingDefer(
            rect, $.NSWindowStyleMaskBorderless, $.NSBackingStoreBuffered, false);
        if (!window) throw Error('Owned native backdrop window unavailable');
        window.releasedWhenClosed = false;
        const colours = [[0.9, 0.1, 0.1], [0.1, 0.7, 0.2], [0.1, 0.2, 0.9]];
        const views = colours.map((colour, index) => {
            const view = $.NSView.alloc.initWithFrame($.NSMakeRect(rect.size.width * index / 3, 0, rect.size.width / 3, rect.size.height));
            view.wantsLayer = true;
            view.layer.backgroundColor = $.NSColor.colorWithCalibratedRedGreenBlueAlpha(...colour, 1).CGColor;
            window.contentView.addSubview(view);
            return view;
        });
        window.orderFrontRegardless;
        window.displayIfNeeded;
        // Owned process blocks in AppKit's actual run loop; Python terminates
        // only this Popen child, retaining the window and view references here.
        $.NSFileHandle.fileHandleWithStandardOutput.writeData($('{"ready":true}\n').dataUsingEncoding($.NSUTF8StringEncoding));
        app.run;
        result = views.length === 3;
        break;
    }
    default: throw Error('Unknown native appearance action');
    }
    return JSON.stringify(result);
}
