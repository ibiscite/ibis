// JXA, loaded by update-e2e.py. Native values stay in this process. Only
// fixed stages, counts, PID and focus category cross the helper boundary.
function macosUpdate(pids, press) {
  const probe = {stage: 'scope', code: null, failed: false, trusted: null,
    windows: 0, pickers: 0, offers: 0, unnamed: 0, sheets: 0, action_matches: 0, attempted: false};
  const answer = (found = null, pressed = false) => ({pid: found ? found.pid : null,
    focus: found ? found.focus : null, pressed, seen: [], probe});
  try {
    if (!pids.length || pids.length > 20 || pids.some(pid => !/^[1-9][0-9]{0,9}$/.test(pid))) throw Error('Invalid scope');
    probe.stage = 'frameworks';
    ObjC.import('ApplicationServices'); ObjC.import('CoreFoundation'); ObjC.import('CoreGraphics');
    probe.trusted = Boolean($.AXIsProcessTrusted());
    if (!probe.trusted) return answer();
    const fail = stage => { probe.stage = stage; throw Error('Native scope changed'); };
    const type = (value, kind) => {
      const expected = kind === 'element' ? $.AXUIElementGetTypeID() : kind === 'array' ? $.CFArrayGetTypeID() :
        kind === 'boolean' ? $.CFBooleanGetTypeID() : $.CFStringGetTypeID();
      if (!value || Number($.CFGetTypeID(value)) !== Number(expected)) throw Error('Unexpected native type');
      return value;
    };
    const attr = (item, name, kind, stage, optional = false) => {
      probe.stage = stage + '-copy';
      const out = Ref();
      const status = $.AXUIElementCopyAttributeValue(item, $(name), out);
      // Unsupported/no-value is ordinary for AXSheets/leaf children. A
      // timeout or any other AX failure cannot supply a partial census.
      if (optional && (status === -25205 || status === -25212)) return null;
      if (status !== 0) throw Object.assign(Error('AX unavailable'), {errorNumber: status});
      probe.stage = stage + '-read';
      const reference = out[0];
      probe.stage = stage + '-bridge';
      return type(ObjC.castRefToObject(reference), kind);
    };
    const text = (item, name, stage, optional = false) => {
      const value = attr(item, name, 'string', stage, optional);
      return value === null ? '' : ObjC.unwrap(value);
    };
    const list = (item, name, stage, limit, optional = false) => {
      const value = attr(item, name, 'array', stage, optional);
      if (value === null) return [];
      const count = Number(value.count);
      if (!Number.isInteger(count) || count < 0 || count > limit) fail('picker-bounds');
      return Array.from({length: count}, (_, i) => type(value.objectAtIndex(i), 'element'));
    };
    const owner = item => {
      probe.stage = 'ax-owner';
      const out = Ref();
      const status = $.AXUIElementGetPid(item, out);
      if (status !== 0) throw Object.assign(Error('AX owner unavailable'), {errorNumber: status});
      probe.stage = 'ax-owner-read';
      return Number(out[0]);
    };
    const same = (a, b) => Boolean(a && b && $.CFEqual(a, b));
    const snapshot = () => {
      probe.windows = probe.pickers = probe.offers = probe.unnamed = probe.sheets = probe.action_matches = 0;
      probe.stage = 'cg-copy';
      const reference = $.CGWindowListCopyWindowInfo(
        $.kCGWindowListOptionOnScreenOnly | $.kCGWindowListExcludeDesktopElements, 0);
      probe.stage = 'cg-bridge';
      const object = ObjC.castRefToObject(reference);
      probe.stage = 'cg-decode';
      const windows = ObjC.deepUnwrap(object);
      if (!Array.isArray(windows)) throw Error('CG census unavailable');
      const visible = [];
      for (const window of windows) {
        if (!pids.includes(String(window.kCGWindowOwnerPID))) continue;
        if (++probe.windows > 20) fail('picker-bounds');
        // Names are optional, including on an NSAlert sheet. Its title is
        // message text; it is not an NSWindow title in the locked rfd backend.
        if (typeof window.kCGWindowName !== 'string') probe.unnamed++;
        if (window.kCGWindowName === 'Ibis') visible.push(window.kCGWindowOwnerPID);
      }
      probe.pickers = visible.length;
      if (visible.length > 1) fail('unique');
      if (!visible.length) return null;
      const pid = visible[0];
      const app = $.AXUIElementCreateApplication(pid);
      const owned = item => { if (owner(item) !== pid) fail('picker-owner'); };
      const native = list(app, 'AXWindows', 'ax-windows', 20);
      const mains = native.filter(window => { owned(window); return text(window, 'AXTitle', 'ax-title', true) === 'Ibis'; });
      if (mains.length > 1) fail('unique');
      if (!mains.length) return null;
      const main = mains[0];
      const sheets = list(main, 'AXSheets', 'ax-sheets', 20, true);
      probe.sheets = sheets.length;
      for (const sheet of sheets) {
        owned(sheet);
        if (text(sheet, 'AXRole', 'ax-role') !== 'AXSheet' || !same(attr(sheet, 'AXParent', 'element', 'ax-parent'), main)) fail('picker-owner');
      }
      // AppKit can expose a sheet in AXWindows as well as AXSheets. Deduplicate
      // native identities, never positional specifiers or matching text.
      const roots = [...sheets];
      for (const window of native) if (!same(window, main) && !roots.some(root => same(root, window))) roots.push(window);
      const offers = [];
      let visited = 0;
      for (const root of roots) {
        owned(root);
        const role = text(root, 'AXRole', 'ax-role');
        if (role !== 'AXWindow' && role !== 'AXSheet') continue;
        if (role === 'AXSheet' && !same(attr(root, 'AXParent', 'element', 'ax-parent'), main)) fail('picker-owner');
        let headings = 0;
        const actions = [], stack = [root];
        while (stack.length) {
          if (++visited > 2000) fail('picker-bounds');
          const item = stack.pop(); owned(item);
          const role = text(item, 'AXRole', 'ax-role');
          if (!same(item, root) && (role === 'AXWindow' || role === 'AXSheet')) continue;
          if (role === 'AXStaticText' && (text(item, 'AXValue', 'ax-value', true) || text(item, 'AXTitle', 'ax-title', true)) === 'An update is available') headings++;
          if (role === 'AXButton' && text(item, 'AXTitle', 'ax-title') === 'Install and Restart') actions.push(item);
          stack.push(...list(item, 'AXChildren', 'ax-children', 2000, true));
          owned(item);
        }
        if (headings > 1 || actions.length > 1) fail('unique');
        if (headings === 1) offers.push({root, button: actions[0] || null});
      }
      probe.offers = offers.length;
      probe.action_matches = offers.reduce((count, offer) => count + Number(offer.button !== null), 0);
      if (offers.length > 1) fail('unique');
      const system = $.AXUIElementCreateSystemWide();
      const focused = attr(system, 'AXFocusedUIElement', 'element', 'ax-focused');
      if (owner(focused) !== pid) return null;
      const focusedWindow = attr(app, 'AXFocusedWindow', 'element', 'ax-window');
      owned(focusedWindow);
      const inside = root => {
        let item = focused;
        for (let depth = 0; item && depth < 40; depth++) {
          owned(item);
          if (same(item, root)) return true;
          item = attr(item, 'AXParent', 'element', 'ax-parent', true);
        }
        if (item) fail('picker-bounds');
        return false;
      };
      if (offers.length) {
        const offer = offers[0];
        if ((!same(focusedWindow, main) && !same(focusedWindow, offer.root)) || !inside(offer.root)) return null;
        if (offer.button && !Boolean(ObjC.unwrap(attr(offer.button, 'AXEnabled', 'boolean', 'ax-enabled')))) offer.button = null;
        return {pid, main, offer: offer.root, button: offer.button, focus: 'offer'};
      }
      return same(focusedWindow, main) && inside(main) ? {pid, main, focus: 'picker'} : null;
    };
    const found = snapshot();
    if (press && found && found.focus === 'offer' && found.button) {
      // Re-enumerate after resolving the action. A late detach or close/reopen
      // cannot turn an old decision into consent for a new native dialog.
      const current = snapshot();
      if (!current || current.pid !== found.pid || current.focus !== 'offer' ||
          !same(current.main, found.main) || !same(current.offer, found.offer) || !same(current.button, found.button)) fail('offer-recheck');
      probe.stage = 'ax-press'; probe.attempted = true;
      const status = $.AXUIElementPerformAction(current.button, $('AXPress'));
      if (status !== 0) throw Object.assign(Error('AXPress refused'), {errorNumber: status});
      probe.stage = 'complete';
      return answer(current, true);
    }
    probe.stage = 'complete';
    return answer(found);
  } catch (error) {
    probe.failed = true;
    probe.code = Number.isInteger(error && error.errorNumber) ? error.errorNumber : null;
    return answer();
  }
}
