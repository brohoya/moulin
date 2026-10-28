// The hero of the website. The sails turn in a light wind. A drag with the mouse turns them faster.
// With the visitor's permission, the loudness of the microphone turns them and fills the bars of the sails.
import { SAIL_FINE, VOICE, barRect, glyphInner, millSvg, sailsSvg, starsSvg, towerParts, windSvg } from './mill.js';
import { WORDMARK } from './wordmark.js';

const hero = document.querySelector('.hero');
const scene = hero.querySelector('.scene');
const ink = hero.querySelector('.ink');
const word = hero.querySelector('.wordmark svg');
const copy = hero.querySelector('.hero-copy');
const listen = document.getElementById('listen');
const note = document.getElementById('listen-note');
const reduced = matchMedia('(prefers-reduced-motion: reduce)');
// Keep this query equal to the query of the stacked hero in site.css.
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
  const c = copy.getBoundingClientRect();
  const lede = copy.querySelector('.lede').getBoundingClientRect();
  const wide = !stacked.matches;
  let R;
  let hub;
  let ground;
  if (wide) {
    // The sails may pass behind the wordmark, but not behind the sentence and the buttons.
    R = Math.min(H * 0.33, W * 0.23);
    let x = Math.max(W * 0.69, lede.right - box.left + 0.92 * (REACH / G.length) * R);
    while (x + 0.5 * (REACH / G.length) * R > W && R > 120) {
      R *= 0.94;
      x = Math.max(W * 0.69, lede.right - box.left + 0.92 * (REACH / G.length) * R);
    }
    hub = [x, H * 0.42];
    ground = Math.min(H - 34, hub[1] + 1.42 * R);
  } else {
    ground = wordTop - 30;
    R = Math.max(90, Math.min(W * 0.4, (ground - 70) / 2.3));
    hub = [W * 0.64, Math.max(62 + (REACH / G.length) * R, ground - 1.3 * R)];
  }
  const text = { x: c.left - box.left - 12, y: c.top - box.top - 12, w: Math.min(c.width, w.width + 40) + 24, h: c.height + 24 };
  return { W, H, R, hub, ground, wide, text, word: { x: w.left - box.left, y: wordTop, w: w.width } };
}

function build() {
  const L = layout();
  if (!L.W || !L.H) return;
  const { W, H, R, hub, ground, wide } = L;
  const s = R / G.length;
  const height = (ground + 12 - hub[1]) / s - 30;
  const t = towerParts(height);
  const at = `translate(${r2(hub[0])} ${r2(hub[1])}) scale(${r2(s * 1000) / 1000})`;
  // The near hill: low at the left on a wide screen, so that the text stays on the sky.
  const left = wide ? H - 8 : ground + 26;
  const near = `M0 ${H} L0 ${r2(left)} C ${r2(W * 0.3)} ${r2(left - (left - ground) * 0.4)}, ${r2(hub[0] - 1.6 * R)} ${r2(ground + 4)}, ${r2(hub[0])} ${r2(ground)} S ${r2(W - 30)} ${r2(ground + 12)}, ${W} ${r2(ground + 22)} L${W} ${H} Z`;
  const farTop = ground - Math.max(36, H * 0.06);
  const far = `M0 ${r2(farTop + 30)} C ${r2(W * 0.22)} ${r2(farTop - 14)}, ${r2(W * 0.46)} ${r2(farTop + 18)}, ${r2(W * 0.6)} ${r2(farTop + 4)} S ${r2(W * 0.9)} ${r2(farTop - 26)}, ${W} ${r2(farTop - 8)} L${W} ${H} L0 ${H} Z`;

  scene.setAttribute('viewBox', `0 0 ${W} ${H}`);
  scene.innerHTML = `<defs>
    <linearGradient id="sky" x1="0" y1="0" x2="0" y2="1"><stop class="stop-1" offset="0"/><stop class="stop-2" offset="0.62"/><stop class="stop-3" offset="1"/></linearGradient>
    <radialGradient id="glow" cx="${r2(hub[0])}" cy="${r2(hub[1])}" r="${r2(Math.max(W, H) * 0.62)}" gradientUnits="userSpaceOnUse"><stop class="glow-stop" offset="0" stop-opacity="0.42"/><stop class="glow-stop" offset="1" stop-opacity="0"/></radialGradient>
    <linearGradient id="near" x1="0" y1="0" x2="0" y2="1"><stop class="near-1" offset="0"/><stop class="near-2" offset="1"/></linearGradient>
    <radialGradient id="halo"><stop offset="0" stop-color="#FF3B30" stop-opacity="0.7"/><stop offset="1" stop-color="#FF3B30" stop-opacity="0"/></radialGradient>
  </defs>
  <rect width="${W}" height="${H}" fill="url(#sky)"/>
  <rect width="${W}" height="${H}" fill="url(#glow)"/>
  <g class="stars">${starsSvg({ width: W, height: H, count: Math.round((W * H) / 26000), avoid: [L.text] })}</g>
  <g class="winds">${windSvg({ width: W, height: H })}</g>
  <path class="far" d="${far}" fill-opacity="0.55"/>
  <g transform="${at}">${millSvg({ id: 'hero', height, g: G, gap: 10, voice: motion.shown })}<circle class="halo" r="110" fill="url(#halo)"/></g>
  <path d="${near}" fill="url(#near)"/>`;

  // The wordmark again, in the color of the sky, where the white mill passes behind the letters.
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
  // The CSS gives the hub a soft glow at night. While the microphone listens, the glow follows the loudness.
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

// ---------- The four states of the menu bar icon. ----------

for (const item of document.querySelectorAll('.states li')) {
  const glyph = item.querySelector('.glyph');
  glyph.innerHTML = `<svg viewBox="0 0 32 32" aria-hidden="true" focusable="false">${glyphInner({ id: `state-${item.dataset.state}`, color: 'currentColor' })}</svg>`;
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
