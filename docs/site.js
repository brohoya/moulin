// The website: one screen. The sails turn in a light wind, and a drag with the mouse turns them faster.
// With the visitor's permission, the loudness of the microphone turns them and fills the bars of the sails.
// Below the words, one tab for each system shows how to install Moulin.
import { SAIL_FINE, VOICE, barRect, millSvg, sailsSvg, towerParts, windSvg } from './mill.js';
import { WORDMARK } from './wordmark.js';

const hero = document.querySelector('.hero');
const scene = hero.querySelector('.scene');
const ink = hero.querySelector('.ink');
const word = hero.querySelector('.wordmark svg');
const words = hero.querySelector('.words');
const listen = document.getElementById('listen');
const note = document.getElementById('listen-note');
const reduced = matchMedia('(prefers-reduced-motion: reduce)');
// Keep this query equal to the query of the stacked page in site.css.
const stacked = matchMedia('(max-width: 979px), (max-aspect-ratio: 5/4)');

const G = SAIL_FINE;
const SLICES = VOICE.length;
const REACH = Math.hypot(G.length, G.width); // The farthest point of a sail from the hub, in sail units.
const IDLE = -12; // Degrees per second: one turn in 30 seconds, counterclockwise.
const STEP_MS = 70; // A new loudness value enters the sails at the hub at this interval.
const r2 = (n) => Math.round(n * 100) / 100;

const view = { W: 0, H: 0, hub: [0, 0], scale: 1, groups: [], bars: [], halo: null };
const motion = {
  angle: 0,
  speed: 0,
  drag: null,
  history: VOICE.map(() => 0), // The target level of each bar, from the tip (oldest) to the hub (newest).
  shown: VOICE.map(() => 0), // The level on screen, which follows the target.
  queue: [], // Values that enter at the hub, one for each step.
  nextStep: 0,
  running: false,
  visible: true,
  last: 0,
};
let mic = null;

// The bars move one slice toward the tip, and the new value enters at the hub.
function feed(value) {
  motion.history.shift();
  motion.history.push(value);
}

// Says the word "moulin" into the sails: the same path as the microphone, with the recorded loudness.
function speakTheWord() {
  motion.queue = [...VOICE];
}

function layout() {
  const box = hero.getBoundingClientRect();
  const w = word.getBoundingClientRect();
  const W = Math.round(box.width);
  const H = Math.round(box.height);
  const wordTop = w.top - box.top;
  // The right edge of the text. The sails may pass behind the wordmark, but not behind the text and the install tabs.
  const right = Math.max(...[...words.querySelectorAll('.lede, .why, .caution, .install')].map((el) => el.getBoundingClientRect().right)) - box.left;
  const wide = !stacked.matches;
  let R;
  let hub;
  let ground;
  if (wide) {
    R = Math.min(H * 0.33, W * 0.23);
    let x = Math.max(W * 0.69, right + 0.92 * (REACH / G.length) * R);
    while (x + 0.5 * (REACH / G.length) * R > W && R > 120) {
      R *= 0.94;
      x = Math.max(W * 0.69, right + 0.92 * (REACH / G.length) * R);
    }
    hub = [x, H * 0.42];
    ground = Math.min(H - 34, hub[1] + 1.42 * R);
  } else {
    ground = wordTop - 30;
    R = Math.max(90, Math.min(W * 0.4, (ground - 70) / 2.3));
    hub = [W * 0.64, Math.max(62 + (REACH / G.length) * R, ground - 1.3 * R)];
  }
  return { W, H, R, hub, ground, wide, word: { x: w.left - box.left, y: wordTop, w: w.width } };
}

function build() {
  const L = layout();
  if (!L.W || !L.H) return;
  const { W, H, R, hub, ground, wide } = L;
  const s = R / G.length;
  const height = Math.max(40, (ground + 12 - hub[1]) / s - 30); // The tower, in sail units, down to the hill.
  const t = towerParts(height);
  const at = `translate(${r2(hub[0])} ${r2(hub[1])}) scale(${r2(s * 1000) / 1000})`;
  // The hill: low at the left on a wide screen, so that the text stays on the plain field.
  const left = wide ? H - 8 : ground + 26;
  const hill = `M0 ${H} L0 ${r2(left)} C ${r2(W * 0.3)} ${r2(left - (left - ground) * 0.4)}, ${r2(hub[0] - 1.6 * R)} ${r2(ground + 4)}, ${r2(hub[0])} ${r2(ground)} S ${r2(W - 30)} ${r2(ground + 12)}, ${W} ${r2(ground + 22)} L${W} ${H} Z`;

  scene.setAttribute('viewBox', `0 0 ${W} ${H}`);
  scene.innerHTML = `<defs>
    <radialGradient id="halo"><stop offset="0" stop-color="#FF3B30" stop-opacity="0.7"/><stop offset="1" stop-color="#FF3B30" stop-opacity="0"/></radialGradient>
  </defs>
  <g class="winds">${windSvg({ width: W, height: H, opacity: [0.18, 0.15, 0.13] })}</g>
  <g transform="${at}">${millSvg({ id: 'hero', height, g: G, gap: 10, voice: motion.shown })}<circle class="halo" r="110" fill="url(#halo)"/></g>
  <path class="hill" d="${hill}"/>`;

  // The wordmark again, in the color of the field, where the white mill passes behind the letters.
  const k = L.word.w / 2837;
  ink.setAttribute('viewBox', `0 0 ${W} ${H}`);
  ink.innerHTML = `<defs><mask id="behind" maskUnits="userSpaceOnUse" x="0" y="0" width="${W}" height="${H}">
    <g transform="${at}" fill="#fff"><path d="${t.tower}"/><path d="${t.cap}"/><g class="sails">${sailsSvg(G, motion.shown, 45)}</g><circle r="32"/></g>
  </mask></defs>
  <g mask="url(#behind)"><g transform="translate(${r2(L.word.x - 30 * k)} ${r2(L.word.y + 756 * k)}) scale(${r2(k * 10000) / 10000})">
    <path class="word-inv" d="${WORDMARK.d}"/><circle cx="${WORDMARK.dot.cx}" cy="${WORDMARK.dot.cy}" r="${WORDMARK.dot.r}" fill="#FF3B30"/>
  </g></g>`;

  Object.assign(view, { W, H, hub, scale: s });
  view.groups = [...scene.querySelectorAll('.sails'), ...ink.querySelectorAll('.sails')];
  view.bars = view.groups.map((g) => [...g.querySelectorAll('.bar')]);
  view.halo = scene.querySelector('.halo');
  draw(true);
}

function draw(bars) {
  const turn = `rotate(${r2(motion.angle % 360)})`;
  for (const g of view.groups) g.setAttribute('transform', turn);
  if (bars) {
    const rects = motion.shown.map((v, i) => barRect(G, SLICES, i, v));
    for (const list of view.bars) {
      list.forEach((bar, j) => {
        const b = rects[j % SLICES];
        bar.setAttribute('x', r2(b.x));
        bar.setAttribute('width', r2(b.w));
      });
    }
  }
  // While the microphone listens, a red glow around the hub follows the loudness.
  if (view.halo) view.halo.style.opacity = mic ? r2(Math.min(1, 0.2 + mic.level * 1.6)) : '';
}

function sampleMic() {
  mic.analyser.getFloatTimeDomainData(mic.data);
  let sum = 0;
  for (const v of mic.data) sum += v * v;
  const level = Math.min(1, Math.sqrt(sum / mic.data.length) * 7); // The same scale as the meter of the app.
  mic.level = Math.max(level, mic.level * 0.86); // A fast rise and a slow fall.
  return mic.level;
}

function frame(now) {
  if (!motion.running) return;
  const dt = motion.last ? Math.min(0.05, (now - motion.last) / 1000) : 0;
  motion.last = now;

  const wind = mic ? sampleMic() : 0;
  if (now >= motion.nextStep) {
    motion.nextStep = now + STEP_MS;
    if (mic) feed(wind);
    else if (motion.queue.length) feed(motion.queue.shift());
  }

  const idle = reduced.matches ? 0 : IDLE;
  const target = idle - wind * 640;
  if (!motion.drag) {
    const rate = Math.abs(target) > Math.abs(motion.speed) ? 5 : 0.9; // The mill speeds up fast and slows down slowly.
    motion.speed += (target - motion.speed) * Math.min(1, dt * rate);
    motion.angle += motion.speed * dt;
  }

  let moving = false;
  motion.shown = motion.shown.map((v, i) => {
    const next = v + (motion.history[i] - v) * Math.min(1, dt * 16);
    if (Math.abs(next - motion.history[i]) > 0.002) moving = true;
    return next;
  });
  draw(moving || mic);

  // With reduced motion, the loop stops when nothing moves.
  const still = reduced.matches && !mic && !motion.drag && !moving && !motion.queue.length && Math.abs(motion.speed) < 0.05;
  if (still || !motion.visible || document.hidden) {
    motion.running = false;
    motion.last = 0;
    return;
  }
  requestAnimationFrame(frame);
}

function wake() {
  if (motion.running || !motion.visible || document.hidden) return;
  motion.running = true;
  requestAnimationFrame(frame);
}

// ---------- Drag with a mouse or a pen. A touch scrolls the page as usual. ----------

const local = (e) => {
  const box = scene.getBoundingClientRect();
  return [e.clientX - box.left - view.hub[0], e.clientY - box.top - view.hub[1]];
};
const onSails = ([x, y]) => Math.hypot(x, y) <= REACH * view.scale;
const degrees = ([x, y]) => (Math.atan2(y, x) * 180) / Math.PI;

scene.addEventListener('pointerdown', (e) => {
  if (e.pointerType === 'touch' || e.button !== 0) return;
  const p = local(e);
  if (!onSails(p)) return;
  e.preventDefault();
  scene.setPointerCapture(e.pointerId);
  scene.classList.add('grabbing');
  motion.drag = { a: degrees(p), t: e.timeStamp, v: 0 };
  wake();
});

scene.addEventListener('pointermove', (e) => {
  const p = local(e);
  if (!motion.drag) {
    scene.classList.toggle('can-grab', e.pointerType !== 'touch' && onSails(p));
    return;
  }
  const a = degrees(p);
  let delta = a - motion.drag.a;
  if (delta > 180) delta -= 360;
  if (delta < -180) delta += 360;
  const dt = Math.max(8, e.timeStamp - motion.drag.t) / 1000;
  motion.angle += delta;
  motion.drag.v = motion.drag.v * 0.5 + (delta / dt) * 0.5;
  motion.drag.a = a;
  motion.drag.t = e.timeStamp;
});

function release() {
  if (!motion.drag) return;
  motion.speed = Math.max(-1440, Math.min(1440, motion.drag.v));
  motion.drag = null;
  scene.classList.remove('grabbing');
}
scene.addEventListener('pointerup', release);
scene.addEventListener('pointercancel', release);
scene.addEventListener('pointerleave', () => scene.classList.remove('can-grab'));

// ---------- The microphone: only the loudness, only in this page. ----------

const LABELS = { off: 'Turn the sails with your voice', on: 'Stop listening' };
const ERRORS = {
  NotAllowedError: 'The browser did not allow the microphone.',
  NotFoundError: 'This device has no microphone.',
};

async function startMic() {
  const stream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true } });
  const context = new AudioContext();
  const analyser = context.createAnalyser();
  analyser.fftSize = 1024;
  context.createMediaStreamSource(stream).connect(analyser);
  mic = { stream, context, analyser, data: new Float32Array(analyser.fftSize), level: 0 };
}

function stopMic() {
  if (!mic) return;
  mic.stream.getTracks().forEach((track) => track.stop());
  mic.context.close();
  mic = null;
  speakTheWord();
}

function setListening(on, message = '') {
  listen.setAttribute('aria-pressed', String(on));
  listen.setAttribute('aria-label', on ? LABELS.on : LABELS.off);
  listen.querySelector('span').textContent = on ? LABELS.on : LABELS.off;
  note.textContent = message;
}

if (navigator.mediaDevices?.getUserMedia) {
  listen.hidden = false;
  listen.addEventListener('click', async () => {
    if (mic) {
      stopMic();
      setListening(false);
      wake();
      return;
    }
    try {
      await startMic();
      setListening(true, 'Speak, or blow. The page measures only the loudness, and no audio leaves your browser.');
      wake();
    } catch (err) {
      setListening(false, ERRORS[err.name] || 'The microphone did not start.');
    }
  });
}

document.addEventListener('visibilitychange', () => {
  if (document.hidden && mic) {
    stopMic();
    setListening(false);
  }
  wake();
});

// ---------- Install: one tab for each system, and a copy button for each command. ----------

const tabs = [...document.querySelectorAll('[role="tab"]')];

function select(tab, focus = false) {
  for (const item of tabs) {
    const on = item === tab;
    item.setAttribute('aria-selected', String(on));
    item.tabIndex = on ? 0 : -1;
    document.getElementById(item.getAttribute('aria-controls')).hidden = !on;
  }
  if (focus) tab.focus();
  build(); // The text can change height, and the mill follows it.
}

tabs.forEach((tab, i) => {
  tab.addEventListener('click', () => select(tab));
  tab.addEventListener('keydown', (e) => {
    const step = { ArrowRight: 1, ArrowLeft: -1 }[e.key];
    if (!step) return;
    e.preventDefault();
    select(tabs[(i + step + tabs.length) % tabs.length], true);
  });
});

// The tab of the visitor's system comes first. A phone gets the macOS tab.
const system = `${navigator.userAgentData?.platform || ''} ${navigator.userAgent}`;
const os = /windows/i.test(system) ? 'windows' : /linux|x11/i.test(system) && !/android/i.test(system) ? 'linux' : 'macos';
select(document.getElementById(`tab-${os}`));

for (const button of document.querySelectorAll('.command .copy')) {
  button.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(button.previousElementSibling.textContent);
      button.textContent = 'Copied';
    } catch {
      window.getSelection().selectAllChildren(button.previousElementSibling); // The visitor can copy the selection.
      button.textContent = 'Selected';
    }
    setTimeout(() => { button.textContent = 'Copy'; }, 1600);
  });
}

// ---------- Start. ----------

new IntersectionObserver(([entry]) => {
  motion.visible = entry.isIntersecting;
  wake();
}).observe(hero);

let pending = 0;
new ResizeObserver(() => {
  cancelAnimationFrame(pending);
  pending = requestAnimationFrame(build);
}).observe(hero);
document.fonts?.ready.then(build);
reduced.addEventListener('change', wake);
stacked.addEventListener('change', build);

if (reduced.matches) {
  motion.history = [...VOICE];
  motion.shown = [...VOICE];
} else {
  motion.nextStep = performance.now() + 500; // The word comes after a short silence.
  speakTheWord();
}
build();
wake();
