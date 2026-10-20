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
console.log('Native HID event-plan contracts passed; no macOS execution claimed.');
