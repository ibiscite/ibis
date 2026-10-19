const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
let nodes, presses, roles, windows;
const app = {bundleIdentifier: 'com.apple.systempreferences', launchDate: {timeIntervalSince1970: 100},
    isTerminated: false, terminate: () => false, forceTerminate: () => { app.isTerminated = true; return true; }};
const process = {windows: () => windows};
const context = {ObjC: {import() {}, unwrap: value => value},
    $: {NSRunningApplication: {runningApplicationWithProcessIdentifier: () => app}},
    Application: () => ({applicationProcesses: {whose: () => () => [process]}})};
vm.createContext(context);
vm.runInContext(fs.readFileSync(`${__dirname}/appearance-macos.js`, 'utf8'), context);
function checkbox(name) {
    let value = false;
    return {role: () => { roles++; return 'AXCheckBox'; }, title: () => name, description: () => name,
        name: () => name, value: () => value,
        actions: {byName: () => ({perform: () => { presses.push(name); value = !value; }})}};
}
const contrast = checkbox('Increase contrast'), transparency = checkbox('Reduce transparency');
const other = checkbox('Unrelated control');
nodes = [contrast, other, transparency]; presses = []; roles = 0;
windows = [{entireContents: () => nodes}];
const identity = {pid: 42, started: 100};
const initial = JSON.parse(context.run(['read', JSON.stringify(identity), '']));
assert.deepEqual(initial, {contrast: false, transparency: false});
const cached = {...identity, controls: {window_count: 1, contrast: {window: 0, index: 0, tree_count: 3},
    transparency: {window: 0, index: 2, tree_count: 3}}};
roles = 0;
context.run(['set', JSON.stringify(cached), JSON.stringify({key: 'transparency', value: true})]);
assert.deepEqual(presses, ['Reduce transparency']);
assert.equal(roles, nodes.length, 'Fresh uniqueness requires every current role, exactly once per node');

nodes = [other, transparency, contrast]; presses = [];
context.run(['set', JSON.stringify(cached), JSON.stringify({key: 'contrast', value: true})]);
assert.deepEqual(presses, ['Increase contrast'], 'A reordered index must never invoke the unrelated control');

nodes = [contrast, other, transparency, checkbox('Reduce transparency')]; presses = [];
assert.throws(() => context.run(['set', JSON.stringify(cached), JSON.stringify({key: 'transparency', value: false})]), /ambiguous/);
assert.deepEqual(presses, [], 'Changed tree count must rediscover and reject duplicate exact controls');

nodes = [contrast, checkbox('Reduce transparency'), transparency]; presses = [];
assert.throws(() => context.run(['set', JSON.stringify(cached), JSON.stringify({key: 'transparency', value: false})]), /ambiguous/);
assert.deepEqual(presses, [], 'A same-count duplicate replacing an unrelated node must prevent any action');

nodes = [contrast, other, transparency];
let secondWindow = [other];
windows = [{entireContents: () => nodes}, {entireContents: () => secondWindow}];
context.run(['read', JSON.stringify(identity), '']);
secondWindow = [checkbox('Reduce transparency')]; presses = [];
assert.throws(() => context.run(['set', JSON.stringify({...cached, controls: {...cached.controls, window_count: 2}}), JSON.stringify({key: 'transparency', value: false})]), /ambiguous/);
assert.deepEqual(presses, [], 'A duplicate in another same-count owned window must prevent any action');

assert.throws(() => context.run(['set', JSON.stringify({...cached, started: 99}), JSON.stringify({key: 'contrast', value: false})]), /identity changed/);
assert.deepEqual(presses, []);
assert.throws(() => context.run(['force-close', JSON.stringify({...identity, started: 99}), '{"restoration_verified":true}']), /identity changed/);
assert.equal(app.isTerminated, false, 'Verified preferences cannot authorize terminating a different process instance');
assert.throws(() => context.run(['force-close', JSON.stringify(identity), '{}']), /verified preference restoration/);
assert.equal(app.isTerminated, false);
assert.equal(context.run(['force-close', JSON.stringify(identity), '{"restoration_verified":true}']), 'true');
assert.equal(app.isTerminated, true);
console.log('Fresh exact control uniqueness, ownership and cleanup contracts passed; no macOS execution claimed.');

let dimensions = [720, 480], resizeCount = 0;
const pickerOwner = {pid: 77, executable: '/owned/ibis'};
const pickerApp = {processIdentifier: 77, isTerminated: false, executableURL: {path: '/owned/ibis'}, launchDate: {timeIntervalSince1970: 200},
    get terminated() { throw Error('Use the established AppKit isTerminated accessor'); }};
const pickerWindow = {name: () => 'Ibis', role: () => 'AXWindow', position: () => [20, 30]};
Object.defineProperty(pickerWindow, 'size', {get: () => () => dimensions, set: value => {resizeCount++; dimensions = value;}});
let pickerWindows = [pickerWindow];
context.$.NSRunningApplication.runningApplicationWithProcessIdentifier = pid => pid === 77 ? pickerApp : null;
context.Application = () => ({applicationProcesses: {whose: () => () => [{windows: () => pickerWindows}]}});
context.$.NSScreen = {mainScreen: {frame: {size: {width: 1600, height: 1000}}}};
let cgOwner = 77;
context.$.CGWindowListCopyWindowInfo = () => [{kCGWindowOwnerPID: cgOwner, kCGWindowNumber: 12, kCGWindowAlpha: 1,
    kCGWindowBounds: {X: 20, Y: 30, Width: dimensions[0], Height: dimensions[1]}}];
context.$.CFArrayGetCount = value => value.length;
context.$.CFArrayGetValueAtIndex = (value, index) => value[index];
context.ObjC.castRefToObject = value => value;
context.ObjC.deepUnwrap = value => value;
const pickerCall = (action, request) => JSON.parse(JSON.stringify(context.pickerAction(action, request)));
// JXA wraps Objective-C nil in a truthy proxy. Reading properties from it
// must not abort the bounded registration wait or authorize another action.
const nilProxy = {isNil: () => true, get isTerminated() { throw Error('nil property access'); }};
let registeredApp = nilProxy;
context.$.NSRunningApplication.runningApplicationWithProcessIdentifier = pid => pid === 77 ? registeredApp : null;
assert.equal(pickerCall('picker-owner', {pid: 77, executable: '/owned/ibis'}), null);
assert.throws(() => pickerCall('picker-frame', {owner: pickerOwner}), /identity/);
registeredApp = Object.create(pickerApp);
Object.defineProperty(registeredApp, 'launchDate', {get: () => { throw Error('Unavailable for direct launch'); }});
assert.deepEqual(pickerCall('picker-owner', {pid: 77, executable: '/owned/ibis'}), pickerOwner, 'Direct launches need no LaunchServices launchDate');
assert.deepEqual(pickerCall('picker-frame', {owner: pickerOwner}).window, 12);
registeredApp = Object.create(pickerApp); registeredApp.executableURL = {isNil: () => true};
assert.equal(pickerCall('picker-owner', {pid: 77, executable: '/owned/ibis'}), null);
assert.throws(() => pickerCall('picker-frame', {owner: pickerOwner}), /identity/);
registeredApp = Object.create(pickerApp); registeredApp.executableURL = {path: '/foreign/ibis'};
assert.throws(() => pickerCall('picker-owner', {pid: 77, executable: '/owned/ibis'}), /identity/);
registeredApp = Object.create(pickerApp); registeredApp.processIdentifier = 88;
assert.throws(() => pickerCall('picker-owner', {pid: 77, executable: '/owned/ibis'}), /identity/);
registeredApp = Object.create(pickerApp); registeredApp.isTerminated = true;
assert.throws(() => pickerCall('picker-owner', {pid: 77, executable: '/owned/ibis'}), /identity/);
registeredApp = pickerApp;
const privateFailure = Object.create(pickerApp);
Object.defineProperty(privateFailure, 'executableURL', {get: () => { throw Error('private path or document'); }});
registeredApp = privateFailure;
assert.deepEqual(JSON.parse(context.run(['picker-owner', '', JSON.stringify(pickerOwner)])),
    {nativeAppearanceFailure: {action: 'picker-owner', stage: 'registration', reason: 'native-call-failed'}});
registeredApp = pickerApp;
assert.deepEqual(pickerCall('picker-owner', {pid: 77, executable: '/owned/ibis'}), pickerOwner);
assert.deepEqual(pickerCall('picker-frame', {owner: pickerOwner}),
    {window: 12, position: [20, 30], size: [720, 480], screen: [1600, 1000]});
assert.equal(pickerCall('picker-size', {owner: pickerOwner, window: 12, size: [840, 560]}), true);
assert.deepEqual(Array.from(dimensions), [840, 560]);
assert.equal(resizeCount, 1);
for (const owner of [{...pickerOwner, pid: 88}, {...pickerOwner, executable: '/foreign/ibis'}]) {
    assert.throws(() => pickerCall('picker-size', {owner, window: 12, size: [720, 480]}), /identity/);
}
assert.throws(() => pickerCall('picker-size', {owner: pickerOwner, window: 13, size: [720, 480]}), /identity/);
for (const size of [[true, 480], [200, 100], [2000, 900], [720], [720, NaN]]) {
    assert.throws(() => pickerCall('picker-size', {owner: pickerOwner, window: 12, size}), /size/);
}
pickerWindows = [pickerWindow, pickerWindow];
assert.throws(() => pickerCall('picker-size', {owner: pickerOwner, window: 12, size: [720, 480]}), /ambiguous/);
pickerWindows = [pickerWindow]; cgOwner = 88;
assert.throws(() => pickerCall('picker-size', {owner: pickerOwner, window: 12, size: [720, 480]}), /ownership/);
assert.equal(resizeCount, 1, 'Foreign, stale, invalid or ambiguous targets must never authorize mutation');
cgOwner = 77; pickerApp.isTerminated = true;
assert.throws(() => pickerCall('picker-size', {owner: pickerOwner, window: 12, size: [720, 480]}), /identity/);
assert.equal(resizeCount, 1, 'A terminated instance must never authorize mutation');
console.log('Owned native resize contracts passed; no macOS execution claimed.');
