// The website: one screen. The sails turn in a gusty wind, and a drag with the mouse turns them faster.
// At the start, the cloth opens on each sail, and then the sails say the word "moulin": they turn with its loudness,
// as they turn with the voice in the app. Below the words: one tab for each system.
import { HILL, SAIL_FINE, VOICE, millSvg, sailsSvg, windSvg } from './mill.js';
import { WORDMARK } from './wordmark.js';

const hero = document.querySelector('.hero');
const scene = hero.querySelector('.scene');
const ink = hero.querySelector('.ink');
const word = hero.querySelector('.wordmark svg');
const words = hero.querySelector('.words');
const reduced = matchMedia('(prefers-reduced-motion: reduce)');
// Keep this query equal to the query of the stacked page in site.css.
const stacked = matchMedia('(max-width: 979px), (max-aspect-ratio: 5/4)');

const G = SAIL_FINE;
const REACH = Math.hypot(G.length, G.width) / G.length; // The farthest point of a sail from the hub, in sail lengths.
const IDLE = -12; // Degrees per second: one turn in 30 seconds, counterclockwise.
const LAND = 96; // On a wide screen, the land below the hill top is at least this tall, so that the credits sit on it.
// The start, in ms: the cloth opens on one sail after the other, and then the sails say the word.
const OPEN = { delay: 400, stagger: 120, duration: 720 };
const WORD = { at: 1300, duration: 900 };
const TAU = 2 * Math.PI;
const r2 = (n) => Math.round(n * 100) / 100;

const view = { W: 0, H: 0, hub: [0, 0], scale: 1, groups: [], cloths: [], winds: null, scaled: false };
const motion = {
  angle: 0,
  speed: 0,
  drag: null,
  intro: !reduced.matches, // The start plays one time, and not with reduced motion.
  start: 0, // The time of the first frame. The start waits until the page is on the screen.
  gust: 0,
  running: false,
  visible: true,
  last: 0,
};

// A slow wind that changes all the time: a value from -1 to 1 at the time t, in seconds.
const gust = (t) => 0.6 * Math.sin((TAU * t) / 9.1) + 0.3 * Math.sin((TAU * t) / 4.7 + 1.7) + 0.1 * Math.sin((TAU * t) / 2.3 + 0.4);

// The loudness of the word at x, from 0 (the start of the word) to 1 (the end).
function loudness(x) {
  const f = Math.max(0, Math.min(1, x)) * (VOICE.length - 1);
  const i = Math.min(VOICE.length - 2, Math.floor(f));
  return VOICE[i] + (VOICE[i + 1] - VOICE[i]) * (f - i);
}

// How far the cloth of sail k is open at the time t of the start (0 to 1). It goes a little past 1 and comes back:
// the wind fills the cloth.
function opened(t, k) {
  const x = Math.max(0, Math.min(1, (t - OPEN.delay - k * OPEN.stagger) / OPEN.duration));
  return 1 + 2.25 * (x - 1) ** 3 + 1.25 * (x - 1) ** 2;
}

function layout() {
  const box = hero.getBoundingClientRect();
  const w = word.getBoundingClientRect();
  const W = Math.round(box.width);
  const H = Math.round(box.height);
  const wordTop = w.top - box.top;
  // The right edge of the text. The sails may pass behind the wordmark, but not behind the text and the tabs.
  const right = Math.max(...[...words.querySelectorAll('.definition, .tagline, .install, .caution')].map((el) => el.getBoundingClientRect().right)) - box.left;
  const wide = !stacked.matches;
  let R;
  let hub;
  let ground;
  if (wide) {
    const y = H * 0.42;
    R = Math.min(H * 0.33, W * 0.23, (H - LAND - y) / HILL);
    let x = Math.max(W * 0.69, right + 0.92 * REACH * R);
    while (x + 0.5 * REACH * R > W && R > 120) {
      R *= 0.94;
      x = Math.max(W * 0.69, right + 0.92 * REACH * R);
    }
    hub = [x, y];
    ground = y + HILL * R;
  } else {
    // The mill stands above the words, and the words sit on the land.
    ground = wordTop - 30;
    R = Math.max(60, Math.min(W * 0.4, (ground - 62) / (HILL + REACH)));
    hub = [W * 0.64, Math.max(8 + REACH * R, ground - HILL * R)];
  }
  return { W, H, R, hub, ground, wide, word: { x: w.left - box.left, y: wordTop, w: w.width } };
}

function build() {
  const L = layout();
  if (!L.W || !L.H) return;
  const { W, H, R, hub, ground, wide } = L;
  const s = R / G.length;
  const height = Math.max(40, (ground + 12 - hub[1]) / s - 30); // The tower, in sail units, 12 px into the hill.
  const at = `translate(${r2(hub[0])} ${r2(hub[1])}) scale(${r2(s * 1000) / 1000})`;
  // The hill: low at the left on a wide screen, so that the text stays on the sky.
  const left = wide ? H - 8 : ground + 26;
  const hill = `M0 ${H} L0 ${r2(left)} C ${r2(W * 0.3)} ${r2(left - (left - ground) * 0.4)}, ${r2(hub[0] - 1.6 * R)} ${r2(ground + 4)}, ${r2(hub[0])} ${r2(ground)}`
    + ` C ${r2(Math.min(W - 40, hub[0] + 0.8 * R))} ${r2(ground - 2)}, ${r2(W - 30)} ${r2(ground + 12)}, ${W} ${r2(ground + 22)} L${W} ${H} Z`;
  // The far hill, behind the mill, in a paler green.
  const far = wide
    ? `M0 ${r2(ground + 40)} C ${r2(W * 0.24)} ${r2(ground - 18)}, ${r2(W * 0.46)} ${r2(ground + 16)}, ${r2(W * 0.6)} ${r2(ground - 14)} S ${r2(W * 0.86)} ${r2(ground - 78)}, ${W} ${r2(ground - 56)} L${W} ${H} L0 ${H} Z`
    : `M0 ${r2(ground - 8)} C ${r2(W * 0.3)} ${r2(ground - 40)}, ${r2(W * 0.55)} ${r2(ground - 4)}, ${r2(W * 0.75)} ${r2(ground - 26)} S ${r2(W * 0.95)} ${r2(ground - 36)}, ${W} ${r2(ground - 32)} L${W} ${H} L0 ${H} Z`;

  scene.setAttribute('viewBox', `0 0 ${W} ${H}`);
  scene.innerHTML = `<g class="winds">${windSvg({ width: W, height: H, opacity: [0.95, 0.85, 0.75] })}</g>
  <path class="far" d="${far}"/>
  <g transform="${at}">${millSvg({ height, g: G })}</g>
  <path class="hill" d="${hill}"/>`;

  // The wordmark again, in the color of the sky, where the green sails pass behind the green letters.
  const k = L.word.w / 2837;
  ink.setAttribute('viewBox', `0 0 ${W} ${H}`);
  ink.innerHTML = `<defs><mask id="behind" maskUnits="userSpaceOnUse" x="0" y="0" width="${W}" height="${H}">
    <g transform="${at}" fill="#fff"><g class="sails">${sailsSvg(G, 45)}</g></g>
  </mask></defs>
  <g mask="url(#behind)"><g transform="translate(${r2(L.word.x - 30 * k)} ${r2(L.word.y + 756 * k)}) scale(${r2(k * 10000) / 10000})">
    <path class="word-inv" d="${WORDMARK.d}"/><circle class="word-dot" cx="${WORDMARK.dot.cx}" cy="${WORDMARK.dot.cy}" r="${WORDMARK.dot.r}"/>
  </g></g>`;

  Object.assign(view, { W, H, hub, scale: s, scaled: false });
  view.groups = [...scene.querySelectorAll('.sails'), ...ink.querySelectorAll('.sails')];
  // Each cloth and the number of its sail (0 to 3), for the start.
  view.cloths = [...scene.querySelectorAll('.cloth'), ...ink.querySelectorAll('.cloth')].map((node) => {
    const sail = node.parentNode;
    return { node, k: [...sail.parentNode.children].indexOf(sail) };
  });
  view.winds = scene.querySelector('.winds');
  draw(performance.now());
}

function draw(now) {
  const turn = `rotate(${r2(motion.angle % 360)})`;
  for (const g of view.groups) g.setAttribute('transform', turn);
  // During the start, the cloth opens from the spar. Before the first frame, the cloth stays closed.
  const t = motion.start ? now - motion.start : 0;
  if (motion.intro && t < OPEN.delay + 3 * OPEN.stagger + OPEN.duration) {
    for (const { node, k } of view.cloths) node.setAttribute('transform', `scale(${opened(t, k).toFixed(4)} 1)`);
    view.scaled = true;
  } else if (view.scaled) {
    for (const { node } of view.cloths) node.removeAttribute('transform');
    view.scaled = false;
  }
  // The wind streaks get stronger with each gust.
  if (view.winds && !reduced.matches) view.winds.style.setProperty('--gust', (0.2 + 0.8 * Math.max(0, Math.min(1, motion.gust + 0.25))).toFixed(3));
}

function frame(now) {
  if (!motion.running) return;
  const dt = motion.last ? Math.min(0.05, (now - motion.last) / 1000) : 0;
  motion.last = now;
  if (!motion.start) motion.start = now;
  const t = now - motion.start;

  let target = 0;
  let rise = 5; // The mill speeds up fast and slows down slowly.
  if (!reduced.matches) {
    motion.gust = gust(now / 1000);
    target = IDLE * (1 + 0.6 * motion.gust);
  }
  if (motion.intro) {
    if (t < WORD.at) {
      target = 0; // The mill waits for the cloth.
      motion.speed = 0;
    }
    const x = (t - WORD.at) / WORD.duration;
    if (x >= 0 && x < 1) {
      target = -(20 + 280 * loudness(x)); // The rule of the pop-up in the app: 20 + 280 x the loudness, in degrees per second.
      rise = 8;
    }
    if (x >= 1) motion.intro = false;
  }
  if (!motion.drag) {
    const rate = Math.abs(target) > Math.abs(motion.speed) ? rise : 0.9;
    motion.speed += (target - motion.speed) * Math.min(1, dt * rate);
    motion.angle += motion.speed * dt;
  }
  draw(now);

  // With reduced motion, the loop stops when nothing moves.
  const still = reduced.matches && !motion.drag && Math.abs(motion.speed) < 0.05;
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
const onSails = ([x, y]) => Math.hypot(x, y) <= REACH * G.length * view.scale;
const degrees = ([x, y]) => (Math.atan2(y, x) * 180) / Math.PI;

scene.addEventListener('pointerdown', (e) => {
  if (e.pointerType === 'touch' || e.button !== 0) return;
  const p = local(e);
  if (!onSails(p)) return;
  e.preventDefault();
  scene.setPointerCapture(e.pointerId);
  scene.classList.add('grabbing');
  motion.intro = false; // A drag ends the start: the cloth opens at once.
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

document.addEventListener('visibilitychange', wake);

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
reduced.addEventListener('change', () => {
  if (reduced.matches) motion.intro = false;
  wake();
});
stacked.addEventListener('change', build);

build();
wake();
