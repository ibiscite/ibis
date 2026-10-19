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
