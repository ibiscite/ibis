const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const context = {ObjC: {import() {}}};
vm.createContext(context);
vm.runInContext(fs.readFileSync(`${__dirname}/smoke-macos.js`, 'utf8'), context);

const fullscreenRead = () => context.readEditorFullscreen(() => { throw Error('Synthetic transition AX getter failure'); });
assert.equal(JSON.stringify(fullscreenRead()), '{"available":false,"fullscreen":null}');
assert.equal(JSON.stringify(context.readEditorFullscreen(() => false)), '{"available":true,"fullscreen":false}');
assert.equal(JSON.stringify(context.readEditorFullscreen(() => true)), '{"available":true,"fullscreen":true}');
let fullscreenReads = 0;
const settlingFullscreen = () => {
    fullscreenReads++;
    if (fullscreenReads === 1) throw Error('Synthetic transition AX getter failure');
    return false;
};
assert.equal(context.readEditorFullscreen(settlingFullscreen).available, false);
assert.equal(JSON.stringify(context.readEditorFullscreen(settlingFullscreen)), '{"available":true,"fullscreen":false}');
assert.equal(fullscreenReads, 2);
for (const malformed of [null, undefined, 0, 'false']) {
    assert.throws(() => context.readEditorFullscreen(() => malformed), /Editor fullscreen accessibility state unavailable/);
}

const staleWindow = () => { throw Error('Error: Error: Invalid index. (-1719)'); };
assert.equal(JSON.stringify(context.readEditorWindowState(staleWindow)), '{"available":false,"fullscreen":null,"visible":false}');
assert.equal(JSON.stringify(context.readEditorWindowState(() => ({available: true, fullscreen: false, visible: true}))),
    '{"available":true,"fullscreen":false,"visible":true}');
for (const fatal of ['osascript is not allowed assistive access. (-1719)', 'A single synthetic editor window is required (found 0)',
    'On-screen window bounds unavailable']) {
    assert.throws(() => context.readEditorWindowState(() => { throw Error(fatal); }), error => error.message === fatal);
}

function fullscreenTransition(states, {desired = true, owner = 7, visible = true, windowCounts = [1]} = {}) {
    let elapsed = 0, reads = 0, observations = 0, windowLists = 0;
    const writes = [];
    const window = {position: () => [10, 20], size: () => [100, 200], attributes: {byName: name => {
        assert.equal(name, 'AXFullScreen');
        return {
            get value() { return () => {
                const state = states[Math.min(reads++, states.length - 1)];
                if (state instanceof Error) throw state;
                return state;
            }; },
            set value(state) { writes.push(state); },
        };
    }}};
    const native = {
        ObjC: {import() {}, deepUnwrap: value => value, castRefToObject: value => value},
        Application: () => ({applicationProcesses: {whose: ({unixId}) => {
            assert.equal(unixId, 7);
            return () => [{windows: () => Array(windowCounts[Math.min(windowLists++, windowCounts.length - 1)]).fill(window)}];
        }}}),
        Date: {now: () => elapsed},
        $: {
            CGWindowListCopyWindowInfo: () => {
                observations++;
                const shown = typeof visible === 'function' ? visible(observations) : visible;
                return [{kCGWindowOwnerPID: owner, kCGWindowAlpha: shown ? 1 : 0,
                    kCGWindowBounds: {X: 10, Y: 20, Width: 100, Height: 200}}];
            },
            CFArrayGetCount: values => values.length,
            CFArrayGetValueAtIndex: (values, index) => values[index],
            NSThread: {sleepForTimeInterval: seconds => { elapsed += seconds * 1000; }},
        },
    };
    vm.createContext(native);
    vm.runInContext(fs.readFileSync(`${__dirname}/smoke-macos.js`, 'utf8'), native);
    return {invoke: () => JSON.parse(native.run(['set-editor-fullscreen', '42', '7', String(desired)])),
        writes, get reads() { return reads; }, get observations() { return observations; },
        get windowLists() { return windowLists; }, get elapsed() { return elapsed; }};
}

// A ready sample followed by a transient negative must not be re-read in a
// separate final guard, which used to reject before the wait budget expired.
for (const desired of [true, false]) {
    const transientFullscreen = fullscreenTransition([!desired, !desired, desired, !desired], {desired});
    assert.deepEqual(transientFullscreen.invoke(), {fullscreen: desired, visible: true});
    assert.equal(transientFullscreen.reads, 3);
    assert.equal(transientFullscreen.observations, 2);
    assert.equal(transientFullscreen.windowLists, 4); // Initial read/write, then one window per poll.
    assert.deepEqual(transientFullscreen.writes, [desired]);
    assert.equal(transientFullscreen.elapsed, 100);
}

const unreadableFullscreen = fullscreenTransition([true, Error('Synthetic AX transition'), false], {desired: false});
assert.deepEqual(unreadableFullscreen.invoke(), {fullscreen: false, visible: true});
assert.equal(unreadableFullscreen.elapsed, 100);
assert.deepEqual(unreadableFullscreen.writes, [false]);
const settlingVisibility = fullscreenTransition([false, true], {visible: observation => observation > 1});
assert.deepEqual(settlingVisibility.invoke(), {fullscreen: true, visible: true});
assert.equal(settlingVisibility.elapsed, 100);

for (const fixture of [
    fullscreenTransition([true, Error('Synthetic unknown AX state')], {desired: false}),
    fullscreenTransition([false]),
    fullscreenTransition([true], {desired: false}),
    fullscreenTransition([true], {visible: false}),
    fullscreenTransition([true], {owner: 42}),
]) {
    assert.throws(fixture.invoke, /Editor fullscreen transition did not complete on screen/);
    assert.equal(fixture.elapsed, 20000);
    assert.equal(fixture.observations, 200); // No extra final sample after the deadline.
    assert.ok(fixture.writes.length <= 1);
}
for (const windowCount of [0, 2]) {
    const invalidWindow = fullscreenTransition([false], {windowCounts: [1, 1, windowCount]});
    assert.throws(invalidWindow.invoke, /A single synthetic editor window is required/);
    assert.equal(invalidWindow.elapsed, 0);
}
assert.throws(fullscreenTransition([false, null]).invoke, /Editor fullscreen accessibility state unavailable/);
assert.throws(fullscreenTransition([false, true], {owner: '7'}).invoke, /Window ownership metadata unavailable/);

const events = [];
context.sendPhysicalChord('ctrl+super+r', (...event) => events.push(event));
assert.deepEqual(events, [
    [59, true, 1 << 18], [55, true, (1 << 18) | (1 << 20)],
    [15, true, (1 << 18) | (1 << 20)], [15, false, (1 << 18) | (1 << 20)],
    [55, false, 1 << 18], [59, false, 0],
]);

const goToFolder = [];
context.sendPhysicalChord('super+shift+g', (...event) => goToFolder.push(event));
assert.deepEqual(goToFolder, [
    [55, true, 1 << 20], [56, true, (1 << 20) | (1 << 17)],
    [5, true, (1 << 20) | (1 << 17)], [5, false, (1 << 20) | (1 << 17)],
    [56, false, 1 << 20], [55, false, 0],
]);

const pastePath = [];
context.sendPhysicalChord('super+v', (...event) => pastePath.push(event));
assert.deepEqual(pastePath, [[55, true, 1 << 20], [9, true, 1 << 20], [9, false, 1 << 20], [55, false, 0]]);

const failed = [];
assert.throws(() => context.sendPhysicalChord('ctrl+shift+space', (code, down, flags) => {
    failed.push([code, down, flags]);
    if (code === 49 && down) throw Error('Synthetic event-creation failure');
}), /Synthetic event-creation failure/);
assert.deepEqual(failed.slice(-3), [[49, false, (1 << 18) | (1 << 17)], [56, false, 1 << 18], [59, false, 0]]);

const cleanup = [];
assert.throws(() => context.sendPhysicalChord('ctrl+super+r', (code, down, flags) => {
    cleanup.push([code, down, flags]);
    if (code === 55 && !down) throw Error('Synthetic modifier-release failure');
}), /Synthetic modifier-release failure/);
assert.deepEqual(cleanup.at(-1), [59, false, 0]);

const text = [];
context.sendUnicodeText('a汉😀', (...event) => text.push(event));
assert.deepEqual(text.map(event => [event[1], event[3], event[3].length]), [
    [true, 'a', 1], [false, 'a', 1], [true, '汉', 1], [false, '汉', 1],
    [true, '😀', 2], [false, '😀', 2],
]);
assert.ok(text.every(event => event[0] === 0 && event[2] === 0));
assert.throws(() => context.sendPhysicalChord('ctrl+unknown', () => assert.fail('Invalid chord emitted an event')), /Unsupported/);

context.Application = () => ({});
context.$ = {AXIsProcessTrusted: () => true, CGPreflightPostEventAccess: () => false};
assert.throws(() => context.run(['probe', '42', '0', '']), /TCC event posting is not granted/);
context.Application = () => ({applicationProcesses: []});
context.$ = {AXIsProcessTrusted: () => true};
let bound = false;
context.ObjC.bindFunction = (name, signature) => {
    assert.equal(name, 'CGPreflightPostEventAccess');
    assert.equal(JSON.stringify(signature), '["bool",[]]');
    bound = true;
    context.$.CGPreflightPostEventAccess = () => true;
};
assert.equal(context.run(['probe', '42', '0', '']), 'true');
assert.equal(bound, true);

function nativeClick(controls, text = 'Typst') {
    const window = {entireContents: () => controls};
    const windows = _selector => [window];
    windows[0] = window;
    const native = {ObjC: {import() {}}, Application: () => ({applicationProcesses: {
        whose: ({unixId}) => { assert.equal(unixId, 42); return () => [{windows}]; },
    }})};
    vm.createContext(native);
    vm.runInContext(fs.readFileSync(`${__dirname}/smoke-macos.js`, 'utf8'), native);
    return () => JSON.parse(native.run(['click', '42', '0', text]));
}

function nativeControl({role = 'AXCheckBox', subrole = 'AXToggle', label = 'Typst', error} = {}) {
    let presses = 0;
    return {role: () => role, subrole: () => subrole, description: () => label,
        name: () => label, title: () => label, value: () => false,
        attributes: {byName: () => ({value: () => label})},
        actions: {byName: name => {
            assert.equal(name, 'AXPress');
            return {perform() { presses++; if (error) throw error; }};
        }}, get presses() { return presses; }};
}

const typstToggle = nativeControl();
const toggleResult = nativeClick([typstToggle])();
assert.equal(toggleResult.pressed, true);
assert.equal(toggleResult.toggle_role, true);
assert.equal(typstToggle.presses, 1);
for (const control of [nativeControl({subrole: ''}), nativeControl({label: 'private Typst suffix'})]) {
    assert.equal(nativeClick([control])().pressed, false);
    assert.equal(control.presses, 0);
}
const duplicates = [nativeControl(), nativeControl()];
assert.throws(nativeClick(duplicates), /ambiguous/);
assert.ok(duplicates.every(control => control.presses === 0));
const excessiveControls = Array.from({length: 4001}, () => nativeControl());
assert.throws(nativeClick(excessiveControls), /traversal bound/);
assert.ok(excessiveControls.every(control => control.presses === 0));
const uncertainToggle = nativeControl({error: Error('Synthetic uncertain press')});
assert.throws(nativeClick([uncertainToggle]), /Synthetic uncertain press/);
assert.equal(uncertainToggle.presses, 1);
const ordinaryButton = nativeControl({role: 'AXButton', subrole: '', label: 'More'});
assert.equal(nativeClick([ordinaryButton], 'More')().pressed, true);
assert.equal(ordinaryButton.presses, 1);
context.$ = {AXIsProcessTrusted: () => true};
context.ObjC.bindFunction = () => { throw Error('Synthetic missing native symbol'); };
assert.throws(() => context.run(['probe', '42', '0', '']), /Native event-posting preflight unavailable/);
context.$ = {CGPreflightPostEventAccess: () => true, CGEventSourceCreate: () => null};
assert.throws(() => context.run(['key', '42', '0', 'escape']), /Native HID event source unavailable/);

const posted = [];
let unicodeBindings = 0;
context.ObjC.bindFunction = (name, signature) => {
    assert.equal(name, 'CGEventKeyboardSetUnicodeString');
    assert.equal(JSON.stringify(signature), '["void",["void *","unsigned long","void *"]]');
    unicodeBindings++;
    context.$.CGEventKeyboardSetUnicodeString = (event, length, bytes) => {
        assert.equal(bytes.length, length * 2);
        event.text = bytes.toString('utf16le');
    };
};
context.$ = Object.assign(character => ({dataUsingEncoding: () => {
    const bytes = Buffer.from(character, 'utf16le');
    return {length: bytes.length, bytes};
}}), {
    CGPreflightPostEventAccess: () => true, CGEventSourceCreate: () => ({}),
    CGEventCreateKeyboardEvent: (_source, code, down) => ({code, down}),
    CGEventSetFlags: (event, flags) => { event.flags = flags; },
    CGEventKeyboardSetUnicodeString: () => { throw Error('Ref has incompatible type'); },
    kCGHIDEventTap: 9,
    CGEventPost: (tap, event) => { assert.equal(tap, 9); posted.push(event); },
});
context.run(['text', '42', '0', '汉😀']);
assert.equal(unicodeBindings, 1);
assert.deepEqual(posted.map(event => [event.text, event.down, event.flags]), [
    ['汉', true, 0], ['汉', false, 0], ['😀', true, 0], ['😀', false, 0],
]);

// Keyboard ownership is the picker window's own AXFocused (AppKit key window),
// never the WebView's DOM focus on the search field (#454).
function pickerKey(windows, {onScreen = true} = {}) {
    const native = {
        ObjC: {import() {}, deepUnwrap: value => value, castRefToObject: value => value},
        Application: () => ({applicationProcesses: {whose: ({unixId}) => {
            assert.equal(unixId, 42);
            return () => [{windows: () => windows}];
        }}}),
        $: {
            CGWindowListCopyWindowInfo: () => (onScreen ? [{kCGWindowOwnerPID: 42, kCGWindowAlpha: 1,
                kCGWindowBounds: {X: 10, Y: 20, Width: 100, Height: 200}}] : []),
            CFArrayGetCount: values => values.length,
            CFArrayGetValueAtIndex: (values, index) => values[index],
        },
    };
    vm.createContext(native);
    vm.runInContext(fs.readFileSync(`${__dirname}/smoke-macos.js`, 'utf8'), native);
    return JSON.parse(native.run(['picker-key', '42', '7', '']));
}
const window = (title, key) => ({title: () => title, description: () => '', name: () => title, value: () => '',
    position: () => [10, 20], size: () => [100, 200],
    attributes: {byName: name => {
        assert.equal(name, 'AXFocused');
        return {value: () => { if (key instanceof Error) throw key; return key; }};
    }}});
assert.equal(pickerKey([window('Ibis', true)]), true);
assert.equal(pickerKey([window('Ibis', false)]), false);
assert.equal(pickerKey([window('Ibis', Error('Synthetic AX read failure'))]), false);
assert.equal(pickerKey([window('Other', true)]), false);
assert.equal(pickerKey([window('Ibis', true)], {onScreen: false}), false);
assert.equal(pickerKey([]), false);

// Snapshot aliases must share one observation of each element's state, while
// separate elements and later polls still get fresh accessibility reads.
function snapshotProbe(descriptions = [{}]) {
    const states = descriptions.map(description => ({names: ['Synthetic title alias', 'Synthetic description alias', 'Search references'],
        role: 'AXComboBox', value: 'smoke099999', focused: true, ...description}));
    const reads = states.map(() => ({role: 0, value: 0, focused: 0}));
    const items = states.map((state, index) => {
        const read = key => {
            reads[index][key]++;
            if (state[key] instanceof Error) throw state[key];
            return state[key];
        };
        return {title: () => state.names[0], description: () => state.names[1], name: () => state.names[2],
            role: () => read('role'), value: () => read('value'),
            attributes: {byName: name => { assert.equal(name, 'AXFocused'); return {value: () => read('focused')}; }}};
    });
    const picker = {title: () => 'Ibis', description: () => '', name: () => 'Ibis', value: () => '',
        entireContents: () => items, position: () => [10, 20], size: () => [100, 200]};
    const windows = _selector => [picker];
    windows[0] = picker;
    const native = {
        ObjC: {import() {}, deepUnwrap: value => value, castRefToObject: value => value},
        Application: () => ({applicationProcesses: {whose: ({unixId}) => {
            assert.equal(unixId, 42); return () => [{windows}];
        }}}),
        $: {CGWindowListCopyWindowInfo: () => [{kCGWindowOwnerPID: 42, kCGWindowAlpha: 1,
            kCGWindowBounds: {X: 10, Y: 20, Width: 100, Height: 200}}],
            CFArrayGetCount: values => values.length,
            CFArrayGetValueAtIndex: (values, index) => values[index]},
    };
    vm.createContext(native);
    vm.runInContext(fs.readFileSync(`${__dirname}/smoke-macos.js`, 'utf8'), native);
    return {states, reads, invoke: () => JSON.parse(native.run(['snapshot', '42', '0', '']))};
}
const aliased = snapshotProbe();
assert.deepEqual(aliased.invoke(), {nodes: aliased.states[0].names.map(name => ({
    name, role: 'AXComboBox', value: 'smoke099999', focused: true})), visible: true, foreground: true});
assert.deepEqual(aliased.reads, [{role: 1, value: 1, focused: 1}]);
aliased.states[0].role = 'AXButton';
aliased.states[0].value = 'new value';
aliased.states[0].focused = false;
assert.deepEqual(aliased.invoke(), {nodes: aliased.states[0].names.map(name => ({
    name, role: 'AXButton', value: 'new value', focused: false})), visible: true, foreground: false});
assert.deepEqual(aliased.reads, [{role: 2, value: 2, focused: 2}]);
const repeated = snapshotProbe([{names: ['Search references', 'Search references', 'Search references']}]);
assert.equal(repeated.invoke().nodes.length, 1);
assert.deepEqual(repeated.reads, [{role: 1, value: 1, focused: 1}]);
const unavailableState = snapshotProbe([{role: Error('Synthetic role failure'), value: Error('Synthetic value failure'),
    focused: Error('Synthetic focus failure')}]);
assert.deepEqual(unavailableState.invoke(), {nodes: unavailableState.states[0].names.map(name => ({
    name, role: '', value: '', focused: false})), visible: true, foreground: false});
assert.deepEqual(unavailableState.reads, [{role: 1, value: 1, focused: 1}]);
const separate = snapshotProbe([{}, {role: 'AXButton', value: 'other', focused: false}]);
assert.deepEqual(separate.invoke().nodes.map(node => node.value), ['smoke099999', 'smoke099999', 'smoke099999', 'other', 'other', 'other']);
assert.deepEqual(separate.reads, [{role: 1, value: 1, focused: 1}, {role: 1, value: 1, focused: 1}]);
const unnamed = snapshotProbe([{names: ['', '', '']}]);
assert.equal(unnamed.invoke().nodes[0].name, '');
assert.deepEqual(unnamed.reads, [{role: 1, value: 1, focused: 1}]);
for (const focused of [undefined, null, 1, 'true']) {
    const nonBoolean = snapshotProbe([{focused}]).invoke();
    assert.ok(nonBoolean.nodes.every(node => node.focused === false));
    assert.equal(nonBoolean.foreground, false);
}
assert.throws(snapshotProbe(Array(4001).fill({})).invoke, /traversal bound/);

console.log('Native HID event-plan contracts passed; no macOS execution claimed.');
