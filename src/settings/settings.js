'use strict';
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWindow } = window.__TAURI__.window;
const { LogicalSize } = window.__TAURI__.dpi;
const settingsWindow = getCurrentWindow();
const $ = (id) => document.getElementById(id);
const IS_MAC = navigator.platform.startsWith('Mac');
const SYMBOLS = IS_MAC ? { Control: '⌃', Alt: '⌥', Shift: '⇧', Command: '⌘' } : { Control: 'Ctrl', Alt: 'Alt', Shift: 'Shift', Super: 'Super' };
const MODIFIER_NAMES = IS_MAC ? 'Control, Option, or Command' : 'Ctrl, Alt, or Super (the Windows key)';
const KEYS = {
  Space: 'Space', Tab: 'Tab', Enter: 'Enter', Backspace: 'Backspace', Delete: 'Delete', Insert: 'Insert', Home: 'Home', End: 'End',
  PageUp: 'PageUp', PageDown: 'PageDown', ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right',
  Minus: '-', Equal: '=', BracketLeft: '[', BracketRight: ']', Backslash: '\\', Semicolon: ';', Quote: "'", Comma: ',',
  Period: '.', Slash: '/', Backquote: '`',
};
const MODE_HINTS = {
  toggle: 'Push the key to start, and push it again to stop.',
  hold: 'Hold the key while you speak, and release it to stop.',
};
let current = null;
let capturing = false;

function pretty(accelerator) {
  const parts = accelerator.split('+');
  return IS_MAC ? parts.map((p) => SYMBOLS[p] || p).join('') : parts.map((p) => SYMBOLS[p] || p).join('+');
}

function keyOf(event) {
  let m;
  if ((m = /^Key([A-Z])$/.exec(event.code))) return m[1];
  if ((m = /^Digit(\d)$/.exec(event.code))) return m[1];
  if ((m = /^Numpad(\d)$/.exec(event.code))) return `num${m[1]}`;
  if (/^F\d{1,2}$/.test(event.code)) return event.code;
  return KEYS[event.code] || null;
}

function render(snapshot) {
  if (!snapshot) return;
  current = snapshot;
  const { settings } = snapshot;
  $('version').textContent = `Version ${snapshot.version}`;
  $('status').textContent = snapshot.status;
  $('notice').hidden = !snapshot.notice;
  $('notice').textContent = snapshot.notice || '';
  const kind = !snapshot.signedIn ? 'warn' : snapshot.state === 'error' ? 'error' : 'ok';
  $('badge').dataset.kind = kind;
  $('badge').textContent = { ok: 'Ready', warn: 'Not signed in', error: 'Problem' }[kind];
  $('account').textContent = snapshot.signedIn ? 'Signed in to ChatGPT' : 'Not signed in';
  $('sign-in').hidden = snapshot.signedIn;
  $('sign-out').hidden = !snapshot.signedIn;
  if (!capturing) $('hotkey').textContent = settings.hotkey ? pretty(settings.hotkey) : 'None';
  $('hotkey-error').hidden = !snapshot.hotkeyError;
  $('hotkey-error').textContent = snapshot.hotkeyError || '';
  for (const button of document.querySelectorAll('[data-mode]')) {
    const on = button.dataset.mode === settings.mode;
    button.setAttribute('aria-checked', String(on));
    button.tabIndex = on ? 0 : -1;
  }
  $('mode-hint').textContent = MODE_HINTS[settings.mode] || MODE_HINTS.toggle;
  for (const key of ['autoPaste', 'openAtLogin', 'showOverlay']) $(key).checked = settings[key];
  $('accessibility').hidden = !(snapshot.platform === 'macos' && settings.autoPaste && !snapshot.accessibility);
  if ([...$('microphone').options].some((o) => o.value === settings.microphoneId)) $('microphone').value = settings.microphoneId;
}

async function update(patch) {
  const result = await invoke('settings_set', { patch });
  render(result);
  return result;
}

function endCapture() {
  capturing = false;
  $('hotkey').classList.remove('capturing');
  invoke('settings_capture', { on: false });
  render(current);
}

$('hotkey').addEventListener('click', () => {
  if (capturing) return endCapture();
  capturing = true;
  invoke('settings_capture', { on: true });
  $('hotkey').classList.add('capturing');
  $('hotkey').textContent = 'Press keys…';
  $('hotkey-error').hidden = true;
});

window.addEventListener('keydown', async (event) => {
  if (!capturing) return;
  event.preventDefault();
  if (event.code === 'Escape') return endCapture();
  const modifiers = [event.ctrlKey && 'Control', event.altKey && 'Alt', event.shiftKey && 'Shift', event.metaKey && (IS_MAC ? 'Command' : 'Super')].filter(Boolean);
  // Backspace or Delete alone: no Moulin shortcut. A system shortcut can run Moulin with --toggle.
  if ((event.code === 'Backspace' || event.code === 'Delete') && modifiers.length === 0) {
    endCapture();
    return update({ hotkey: '' });
  }
  const key = keyOf(event);
  if (!key) return; // Only modifiers so far.
  if (!/^F\d{1,2}$/.test(key) && !modifiers.some((m) => m !== 'Shift')) {
    $('hotkey-error').hidden = false;
    $('hotkey-error').textContent = `Use a function key (F1 to F24), or add ${MODIFIER_NAMES}.`;
    return;
  }
  endCapture();
  await update({ hotkey: [...modifiers, key].join('+') });
});

window.addEventListener('blur', () => { if (capturing) endCapture(); });

for (const key of ['autoPaste', 'openAtLogin', 'showOverlay']) {
  $(key).addEventListener('change', (event) => update({ [key]: event.target.checked }));
}

$('microphone').addEventListener('change', (event) => update({ microphoneId: event.target.value }));
for (const button of document.querySelectorAll('[data-mode]')) {
  button.addEventListener('click', () => update({ mode: button.dataset.mode }));
}
$('open-accessibility').addEventListener('click', () => invoke('open_accessibility'));

$('sign-in').addEventListener('click', async () => {
  $('sign-in').disabled = true;
  render(await invoke('account_sign_in'));
  $('sign-in').disabled = false;
});

// Moulin asks for a confirmation in a dialog of the system. The result is null if the user cancels.
$('sign-out').addEventListener('click', async () => {
  render(await invoke('account_sign_out'));
});

// Moulin lists the input devices of the system. The list loads again when the window gets the focus,
// for example after the user plugs in a headset.
async function loadMicrophones() {
  let devices = [];
  try {
    devices = await invoke('microphones');
  } catch {
    // The list stays at "System default".
  }
  $('microphone').replaceChildren(
    new Option('System default', ''),
    ...devices.map((d, i) => new Option(d.label || `Microphone ${i + 1}`, d.id)),
  );
  $('microphone').value = '';
  render(current);
}

// The window takes the height of its content, so that no empty space stays at the bottom. The content changes
// its height, for example when a shortcut error shows.
function fit() {
  return settingsWindow.setSize(new LogicalSize(window.innerWidth, Math.ceil(document.body.getBoundingClientRect().height)));
}

// No menu of the web view (it has "Reload") on a right-click.
window.addEventListener('contextmenu', (event) => event.preventDefault());
window.addEventListener('focus', loadMicrophones);
listen('status', (event) => render(event.payload));
new ResizeObserver(fit).observe(document.body);
// Moulin creates the window hidden. The page shows it when the content and the size are ready, so that the window
// never shows empty and never changes its size at the start.
invoke('settings_get').then(render).finally(async () => {
  await fit();
  await settingsWindow.show();
  await settingsWindow.setFocus();
  loadMicrophones();
});
