'use strict';
const api = window.moulin;
const $ = (id) => document.getElementById(id);
const IS_MAC = navigator.platform.startsWith('Mac');
const SYMBOLS = IS_MAC ? { Control: '⌃', Alt: '⌥', Shift: '⇧', Command: '⌘' } : { Control: 'Ctrl', Alt: 'Alt', Shift: 'Shift', Super: 'Super' };
const KEYS = {
  Space: 'Space', Tab: 'Tab', Enter: 'Enter', Backspace: 'Backspace', Delete: 'Delete', Insert: 'Insert', Home: 'Home', End: 'End',
  PageUp: 'PageUp', PageDown: 'PageDown', ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right',
  Minus: '-', Equal: '=', BracketLeft: '[', BracketRight: ']', Backslash: '\\', Semicolon: ';', Quote: "'", Comma: ',',
  Period: '.', Slash: '/', Backquote: '`',
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
  $('version').textContent = `· v${snapshot.version}`;
  $('status').textContent = snapshot.status;
  const kind = !snapshot.signedIn ? 'warn' : snapshot.state === 'error' ? 'error' : 'ok';
  $('badge').dataset.kind = kind;
  $('badge').textContent = { ok: 'Ready', warn: 'Not signed in', error: 'Problem' }[kind];
  $('account').textContent = snapshot.signedIn ? 'Signed in to ChatGPT' : 'Not signed in';
  $('sign-in').hidden = snapshot.signedIn;
  $('sign-out').hidden = !snapshot.signedIn;
  if (!capturing) $('hotkey').textContent = pretty(settings.hotkey);
  $('hotkey-error').hidden = !snapshot.hotkeyError;
  $('hotkey-error').textContent = snapshot.hotkeyError || '';
  for (const key of ['autoPaste', 'openAtLogin', 'showOverlay']) $(key).checked = settings[key];
  $('accessibility').hidden = !(snapshot.platform === 'darwin' && settings.autoPaste && !snapshot.accessibility);
  if ([...$('microphone').options].some((o) => o.value === settings.microphoneId)) $('microphone').value = settings.microphoneId;
}

async function update(patch) {
  const result = await api.set(patch);
  render(result);
  return result;
}

function endCapture() {
  capturing = false;
  $('hotkey').classList.remove('capturing');
  api.capture(false);
  render(current);
}

$('hotkey').addEventListener('click', () => {
  if (capturing) return endCapture();
  capturing = true;
  api.capture(true);
  $('hotkey').classList.add('capturing');
  $('hotkey').textContent = 'Press keys…';
  $('hotkey-error').hidden = true;
});

window.addEventListener('keydown', async (event) => {
  if (!capturing) return;
  event.preventDefault();
  if (event.code === 'Escape') return endCapture();
  const key = keyOf(event);
  if (!key) return; // Only modifiers so far.
  const modifiers = [event.ctrlKey && 'Control', event.altKey && 'Alt', event.shiftKey && 'Shift', event.metaKey && (IS_MAC ? 'Command' : 'Super')].filter(Boolean);
  if (!/^F\d{1,2}$/.test(key) && !modifiers.some((m) => m !== 'Shift')) {
    $('hotkey-error').hidden = false;
    $('hotkey-error').textContent = 'Use a function key (F1 to F24), or add Control, Option (Alt), or Command.';
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
$('open-accessibility').addEventListener('click', () => api.openAccessibility());

$('sign-in').addEventListener('click', async () => {
  $('sign-in').disabled = true;
  render(await api.signIn());
  $('sign-in').disabled = false;
});

$('sign-out').addEventListener('click', async () => {
  if (!window.confirm('Sign out of ChatGPT in Moulin? Moulin deletes its cookies for chatgpt.com.')) return;
  render(await api.signOut());
});

async function loadMicrophones() {
  let devices = [];
  try {
    devices = (await navigator.mediaDevices.enumerateDevices())
      .filter((d) => d.kind === 'audioinput' && d.deviceId !== 'default' && d.deviceId !== 'communications');
  } catch {
    // The list stays at "System default".
  }
  $('microphone').replaceChildren(
    new Option('System default', ''),
    ...devices.map((d, i) => new Option(d.label || `Microphone ${i + 1}`, d.deviceId)),
  );
  $('microphone').value = '';
  render(current);
}

navigator.mediaDevices.addEventListener('devicechange', loadMicrophones);
api.onStatus(render);
api.get().then((snapshot) => {
  render(snapshot);
  loadMicrophones();
});
