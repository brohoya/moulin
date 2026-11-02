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

// The shortcut shows as keys of the keyboard: one key for each part, for example Ctrl, Shift, and Space.
function showHotkey(accelerator) {
  const keys = accelerator ? accelerator.split('+').map((p) => Object.assign(document.createElement('kbd'), { textContent: SYMBOLS[p] || p })) : ['None'];
  $('hotkey').classList.toggle('empty', !accelerator);
  $('hotkey').replaceChildren(...keys);
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
  if (!capturing) showHotkey(settings.hotkey);
  $('hotkey-error').hidden = !snapshot.hotkeyError;
  $('hotkey-error').textContent = snapshot.hotkeyError || '';
  for (const button of document.querySelectorAll('[data-mode]')) {
    const on = button.dataset.mode === settings.mode;
    button.setAttribute('aria-checked', String(on));
    button.tabIndex = on ? 0 : -1;
  }
  $('mode-hint').textContent = MODE_HINTS[settings.mode] || MODE_HINTS.toggle;
  for (const button of document.querySelectorAll('[data-media]')) {
    const on = button.dataset.media === settings.media;
    button.setAttribute('aria-checked', String(on));
    button.tabIndex = on ? 0 : -1;
  }
  // The amount, and the "by" at the end of the sentence, show only in "lower" mode. A button at its limit does
  // nothing, so it shows as disabled.
  $('media-by').hidden = $('media-lower').hidden = settings.media !== 'lower';
  $('media-amount').textContent = `${amount(settings)}%`;
  for (const button of document.querySelectorAll('[data-step]')) button.disabled = nextAmount(button) === amount(settings);
  for (const key of ['autoPaste', 'openAtLogin', 'showOverlay']) $(key).checked = settings[key];
  $('accessibility').hidden = !(snapshot.platform === 'macos' && settings.autoPaste && !snapshot.accessibility);
  showMicrophone(settings.microphoneId);
}

// How much the "lower" mode lowers the volume, in percent. The settings keep the volume that stays.
function amount(settings) {
  return 100 - settings.mediaVolume;
}

// The amount after a push of a button of the stepper: a step of 10, from 10 % to 100 % (silence).
function nextAmount(button) {
  return Math.min(100, Math.max(10, amount(current.settings) + Number(button.dataset.step)));
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
  $('hotkey').classList.remove('empty');
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

for (const button of document.querySelectorAll('[data-mode]')) {
  button.addEventListener('click', () => update({ mode: button.dataset.mode }));
}
for (const button of document.querySelectorAll('[data-media]')) {
  button.addEventListener('click', () => update({ media: button.dataset.media }));
}
for (const button of document.querySelectorAll('[data-step]')) {
  button.addEventListener('click', () => update({ mediaVolume: 100 - nextAmount(button) }));
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

// The list of microphones. The page draws the list, because a <select> shows the menu of the system, which looks
// different on each system. The focus stays on the field, and the keys move a highlight in the list, as in the
// "select-only combobox" of the ARIA Authoring Practices.
const field = $('microphone');
const list = $('microphones');
let active = null;

// The field shows the chosen microphone. A microphone that is not in the list (for example, an unplugged headset)
// shows as "System default", because the recording then uses the default microphone.
function showMicrophone(id) {
  const options = [...list.children];
  const chosen = options.find((o) => o.dataset.id === id) || options[0];
  if (!chosen) return;
  for (const option of options) option.setAttribute('aria-selected', String(option === chosen));
  field.textContent = chosen.textContent;
}

function highlight(option, withKeys) {
  active?.classList.remove('active');
  active = option;
  active.classList.add('active');
  list.classList.toggle('keys', withKeys);
  field.setAttribute('aria-activedescendant', active.id);
  if (withKeys) active.scrollIntoView({ block: 'nearest' });
}

function openList(withKeys) {
  if (!list.firstElementChild) return; // The list is not loaded yet.
  // The list stays in the window: it gets the height of the room under the field.
  list.style.maxHeight = `${innerHeight - field.getBoundingClientRect().bottom - 16}px`;
  list.hidden = false;
  field.setAttribute('aria-expanded', 'true');
  highlight(list.querySelector('[aria-selected="true"]') || list.firstElementChild, withKeys);
}

function closeList() {
  list.hidden = true;
  field.setAttribute('aria-expanded', 'false');
  field.removeAttribute('aria-activedescendant');
}

function choose(option) {
  closeList();
  if (option.dataset.id !== current?.settings.microphoneId) update({ microphoneId: option.dataset.id });
}

// A press on the list keeps the focus on the field, so that the list stays open until the click.
list.addEventListener('mousedown', (event) => event.preventDefault());
field.addEventListener('click', () => {
  if (list.hidden) openList(false);
  else closeList();
});
// The list closes with no change when the focus goes away: Tab, a click in another place, or another window.
field.addEventListener('blur', closeList);
list.addEventListener('mousemove', (event) => {
  const option = event.target.closest('[role="option"]');
  if (option && (option !== active || list.classList.contains('keys'))) highlight(option, false);
});
list.addEventListener('click', (event) => {
  const option = event.target.closest('[role="option"]');
  if (option) choose(option);
});
// The arrows, Home, and End move the highlight. Enter or Space chooses the microphone, and Escape closes the list.
// When the list is closed, these keys open it.
field.addEventListener('keydown', (event) => {
  const step = { ArrowDown: 1, ArrowUp: -1, Home: -Infinity, End: Infinity }[event.key];
  const press = event.key === 'Enter' || event.key === ' ';
  if (list.hidden) {
    if (step === undefined && !press) return;
    openList(true);
  } else if (step !== undefined) {
    const options = [...list.children];
    highlight(options[Math.min(Math.max(options.indexOf(active) + step, 0), options.length - 1)], true);
  } else if (press) {
    choose(active);
  } else if (event.key === 'Escape') {
    closeList();
  } else {
    return;
  }
  event.preventDefault();
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
  const items = [{ id: '', label: 'System default' }, ...devices.map((d, i) => ({ id: d.id, label: d.label || `Microphone ${i + 1}` }))];
  closeList();
  list.replaceChildren(...items.map((item, i) => {
    const option = Object.assign(document.createElement('div'), { id: `microphone-${i}`, textContent: item.label });
    option.setAttribute('role', 'option');
    option.dataset.id = item.id;
    return option;
  }));
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
