import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { runInNewContext } from "node:vm";

const source = readFileSync(new URL("./update-macos.js", import.meta.url), "utf8");

function desktop(scenario = "sheet") {
  let scans = 0;
  const state = { visible: scenario !== "absent", presses: 0 };
  const scalar = (type, value) => ({ type, value });
  const array = (value) => ({ type: 19, value, count: value.length, objectAtIndex: (i) => value[i] });
  const element = (role, title = "") => ({ type: 81, owner: 42, role, title, children: [] });
  const app = element("AXApplication"), system = element("AXSystemWide");
  const main = element("AXWindow", "Ibis"), offer = element(scenario === "standalone" ? "AXWindow" : "AXSheet");
  const button = element("AXButton", "Install and Restart");
  const label = element("AXStaticText");
  label.value = "An update is available";
  main.parent = app; offer.parent = scenario === "standalone" ? app : main;
  button.parent = offer; label.parent = offer; offer.children = [label, button];
  const replacement = element("AXSheet");
  replacement.parent = main;
  replacement.children = [label, element("AXButton", "Install and Restart")];
  const focus = () => state.visible ? button : main;
  const $ = (value) => value;
  Object.assign($, {
    kCGWindowListOptionOnScreenOnly: 1, kCGWindowListExcludeDesktopElements: 16,
    AXIsProcessTrusted: () => scenario !== "untrusted",
    CGWindowListCopyWindowInfo(options, relative) {
      assert.equal(options, 17); assert.equal(relative, 0); scans++;
      if (scenario === "cg-io") return null;
      const picker = { kCGWindowOwnerPID: 42, kCGWindowName: "Ibis" };
      if (scenario === "hidden") return array([]);
      if (scenario === "unnamed-main") delete picker.kCGWindowName;
      // A sheet has no CG name; this must never erase the known picker.
      return array([picker, { kCGWindowOwnerPID: 42 }]);
    },
    CFGetTypeID: (value) => value.type,
    AXUIElementGetTypeID: () => 81, CFStringGetTypeID: () => 7,
    CFArrayGetTypeID: () => 19, CFBooleanGetTypeID: () => 21,
    CFEqual: (a, b) => a === b,
    AXUIElementCreateSystemWide: () => system,
    AXUIElementCreateApplication: (pid) => { assert.equal(pid, 42); return app; },
    AXUIElementGetPid(item, out) { out[0] = item.owner; return 0; },
    AXUIElementCopyAttributeValue(item, name, out) {
      if (scenario === "io" && item === offer) return -25204;
      if (scenario === "foreign" && item === button) item.owner = 99;
      if (scenario === "owner-changed" && scans > 1 && item === offer) item.owner = 99;
      const current = scenario === "reopened" && scans > 1 ? replacement : offer;
      const windows = [main, ...(state.visible && scenario === "standalone" ? [current] : [])];
      const sheets = state.visible && scenario !== "standalone" ? [current] : [];
      if (scenario === "duplicate") sheets.push(current);
      const attributes = {
        AXRole: scalar(7, item.role), AXTitle: scalar(7, item.title),
        AXValue: scalar(7, scenario === "wrong-heading" ? "Other" : item.value || ""),
        AXEnabled: scalar(21, scenario !== "disabled"),
        AXChildren: array(item.children), AXParent: item.parent,
        AXWindows: item === app ? array(windows) : undefined,
        AXSheets: item === main ? array(sheets) : undefined,
        AXFocusedUIElement: item === system ? focus() : undefined,
        AXFocusedWindow: item === app ? (scenario === "standalone" && state.visible ? offer : main) : undefined,
      };
      if (scenario === "wrong-button" && item === button && name === "AXTitle") attributes.AXTitle = scalar(7, "Later");
      if (scenario === "duplicate-button" && item === offer && name === "AXChildren") attributes.AXChildren = array([label, button, button]);
      if (scenario === "detached" && scans > 1 && item === offer && name === "AXChildren") attributes.AXChildren = array([label]);
      if (scenario === "unfocused" && name === "AXFocusedUIElement") attributes.AXFocusedUIElement = element("AXButton");
      if (scenario === "wrong-parent" && item === offer && name === "AXParent") attributes.AXParent = app;
      if (scenario === "wrong-type" && name === "AXFocusedUIElement") attributes.AXFocusedUIElement = scalar(7, "private-sentinel");
      if (scenario === "unknown-children" && item === offer && name === "AXChildren") return -25204;
      out[0] = attributes[name];
      return out[0] === undefined ? -25205 : 0;
    },
    AXUIElementPerformAction(item, action) {
      assert.equal(item, button); assert.equal(action, "AXPress"); state.presses++;
      if (scenario === "uncertain") throw Error("private-sentinel");
      return scenario === "refused" ? -25204 : 0;
    },
  });
  const ObjC = { import() {}, castRefToObject(value) { if (!value) throw Error("private-sentinel"); return value; },
    deepUnwrap: (value) => value.value, unwrap: (value) => value.value };
  const invoke = runInNewContext(source + "\n(pids, press) => macosUpdate(pids, press)", { $, ObjC, Ref: () => [] });
  return { state, run: (press = false) => JSON.parse(JSON.stringify(invoke(["42"], press))) };
}

test("an anonymous attached sheet and a separate alert each prove one owned native action", () => {
  for (const scenario of ["sheet", "standalone"]) {
    const native = desktop(scenario);
    const result = native.run(true);
    assert.equal(result.pid, 42); assert.equal(result.focus, "offer");
    assert.equal(result.pressed, true, JSON.stringify(result));
    assert.equal(native.state.presses, 1);
    assert.equal(result.probe.unnamed, 1);
  }
});

test("absence, hidden picker, missing identity, wrong meaning and missing focus cannot authorize an action", () => {
  for (const scenario of ["absent", "hidden", "unnamed-main", "wrong-heading", "wrong-button", "disabled", "unfocused", "untrusted"]) {
    const native = desktop(scenario);
    assert.equal(native.run(true).pressed, false, scenario);
    assert.equal(native.state.presses, 0, scenario);
  }
});

test("ambiguity, foreign owners, failed I/O and delayed A after B fail before activation", () => {
  for (const scenario of ["duplicate", "duplicate-button", "foreign", "io", "cg-io", "wrong-parent", "wrong-type", "unknown-children", "owner-changed", "reopened", "detached"]) {
    const native = desktop(scenario);
    const result = native.run(true);
    assert.equal(result.probe.failed, true, scenario);
    assert.equal(result.pressed, false, scenario); assert.equal(native.state.presses, 0, scenario);
    assert.ok(!JSON.stringify(result).includes("private-sentinel"), scenario);
  }
});

test("a late sheet is observed fresh after absence without reopening the app", () => {
  const native = desktop("absent");
  assert.equal(native.run().focus, "picker");
  native.state.visible = true;
  assert.equal(native.run(true).pressed, true);
  assert.equal(native.state.presses, 1);
});

test("a refused or uncertain AXPress is reported as one attempted action", () => {
  for (const scenario of ["refused", "uncertain"]) {
    const native = desktop(scenario);
    const result = native.run(true);
    assert.equal(result.probe.failed, true);
    assert.equal(result.probe.attempted, true);
    assert.equal(native.state.presses, 1);
    assert.ok(!JSON.stringify(result).includes("private-sentinel"));
  }
});
