// Finds the screen of the pop-up and the screen that the user works on (macOS only, run by osascript, see tools.rs).
// The script shares no code with Moulin. The user works on the screen with the largest part of the front window of
// the active app, else on the screen with the pointer. The frames count y down from the top of the main screen.
ObjC.import('AppKit');
ObjC.import('CoreGraphics');
function run(argv) {
  const pid = Number(argv[0]);
  const screens = $.NSScreen.screens;
  const mainHeight = screens.objectAtIndex(0).frame.size.height;
  const flip = (f) => ({ x: f.origin.x, y: mainHeight - f.origin.y - f.size.height, w: f.size.width, h: f.size.height });
  const all = [];
  for (let i = 0; i < screens.count; i++) {
    const s = screens.objectAtIndex(i);
    all.push({ name: s.localizedName.js, frame: flip(s.frame), visible: flip(s.visibleFrame) });
  }
  const overlap = (a, b) => Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x)) * Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
  const largest = (rect) => all.reduce((best, s) => (overlap(s.frame, rect) > overlap(best.frame, rect) ? s : best));
  const inside = (p, r) => p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h;
  const windows = ObjC.castRefToObject($.CGWindowListCopyWindowInfo($.kCGWindowListOptionOnScreenOnly | $.kCGWindowListExcludeDesktopElements, 0));
  const number = (w, key) => w.objectForKey(key).intValue;
  const bounds = (w) => {
    const b = w.objectForKey('kCGWindowBounds');
    return { x: b.objectForKey('X').doubleValue, y: b.objectForKey('Y').doubleValue, w: b.objectForKey('Width').doubleValue, h: b.objectForKey('Height').doubleValue };
  };
  const front = $.NSWorkspace.sharedWorkspace.frontmostApplication;
  let frontWindow = null;
  let popup = null;
  for (let i = 0; i < windows.count; i++) {
    const w = windows.objectAtIndex(i);
    const owner = number(w, 'kCGWindowOwnerPID');
    if (!frontWindow && owner === front.processIdentifier && number(w, 'kCGWindowLayer') === 0) frontWindow = bounds(w);
    if (!popup && owner === pid && number(w, 'kCGWindowLayer') >= 1000) popup = bounds(w);
  }
  if (!popup) return JSON.stringify({ error: 'the pop-up is not on the screen' });
  const mouse = $.NSEvent.mouseLocation;
  const pointer = { x: mouse.x, y: mainHeight - mouse.y };
  const byWindow = frontWindow && overlap(largest(frontWindow).frame, frontWindow) > 0 ? largest(frontWindow) : null;
  const expected = byWindow || all.find((s) => inside(pointer, s.frame)) || all[0];
  const screen = largest(popup);
  return JSON.stringify({
    screens: all.map((s) => s.name),
    activeApp: front.localizedName.js,
    by: byWindow ? 'front window' : 'pointer',
    expectedScreen: expected.name,
    popupScreen: screen.name,
    popup,
    offCenter: popup.x + popup.w / 2 - (screen.visible.x + screen.visible.w / 2),
    bottomGap: screen.visible.y + screen.visible.h - (popup.y + popup.h),
  });
}
