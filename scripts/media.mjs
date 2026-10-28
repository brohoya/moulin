// The images of the website and the README, made from the shapes in docs/mill.js and docs/wordmark.js.
// scripts/make-icons.mjs writes them to docs/media. The animations are CSS in the SVG files, so they also play in a README.
import { COLORS, SAIL_FINE, glyphInner, iconSvg, millSvg, windSvg } from '../docs/mill.js';
import { WORDMARK } from '../docs/wordmark.js';

const C = COLORS;
const r2 = (n) => Math.round(n * 100) / 100;
const FONT = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Ubuntu, 'Helvetica Neue', Arial, sans-serif";

function wordmark({ x, y, size, color = C.white, dot = C.signal }) {
  const { d, dot: o } = WORDMARK;
  return `<g transform="translate(${x} ${y}) scale(${r2(size / 1000)})"><path d="${d}" fill="${color}"/><circle cx="${o.cx}" cy="${o.cy}" r="${o.r}" fill="${dot}"/></g>`;
}

// A wide banner in flat colors: the mill on a hill at the right, the wordmark at the left. Used for the README and the social card.
export function heroSvg({ width = 1280, height = 560, hub = [904, 274], scale = 0.74, word = { x: 96, y: 332, size: 180 }, round = 28, animate = true } = {}) {
  const base = height - 30;
  const tower = r2((base - hub[1]) / scale - 30);
  const hill = `M0 ${height} L0 ${r2(base + 22)} C ${r2(width * 0.3)} ${r2(base + 10)}, ${r2(hub[0] - 300)} ${r2(base - 4)}, ${hub[0]} ${r2(base - 2)} S ${r2(width - 40)} ${r2(base + 6)}, ${width} ${r2(base + 12)} L${width} ${height} Z`;
  const style = animate ? `<style>
  .sails { transform-box: fill-box; transform-origin: center; animation: turn 32s linear infinite; }
  .wind { fill: none; stroke: #fff; stroke-width: 1.6; stroke-linecap: round; animation: blow linear infinite; }
  @keyframes turn { to { transform: rotate(-360deg); } }
  @keyframes blow { from { stroke-dashoffset: 0; } to { stroke-dashoffset: var(--run); } }
  @media (prefers-reduced-motion: reduce) { .sails, .wind { animation: none; } }
</style>` : '<style>.wind { fill: none; stroke: #fff; stroke-width: 1.6; stroke-linecap: round; }</style>';
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}" role="img" aria-label="Moulin">
<title>Moulin</title>
${style}
<defs><clipPath id="banner"><rect width="${width}" height="${height}" rx="${round}"/></clipPath></defs>
<g clip-path="url(#banner)">
  <rect width="${width}" height="${height}" fill="${C.green}"/>
  ${windSvg({ width, height, opacity: [0.2, 0.16, 0.14] })}
  <path d="${hill}" fill="${C.greenDeep}"/>
  <g transform="translate(${hub[0]} ${hub[1]}) scale(${scale})">${millSvg({ id: 'banner', height: tower, g: SAIL_FINE, gap: 10 })}</g>
  ${wordmark(word)}
</g>
</svg>`;
}

// The social card for GitHub and for link previews (1280 x 640).
export const socialSvg = () => heroSvg({ width: 1280, height: 640, hub: [900, 304], scale: 0.8, word: { x: 100, y: 372, size: 184 }, round: 0, animate: false });

// The favicon: the icon without its margin. square: a full square for the iOS home screen, which cuts its own shape.
export function faviconSvg({ square = false } = {}) {
  return iconSvg({ flat: true, square });
}

// ---------- The demo: one dictation in a note, with the menu bar icon, the pop-up, and the key. ----------

const T = 12; // seconds for one loop
const pct = (t) => `${r2((t / T) * 100)}%`;

// Keyframes that show an element between the times of each window, with a fade at each end.
function shown(name, windows, fade = 0.15) {
  const frames = [`0% { opacity: 0; }`];
  for (const [a, b] of windows) {
    frames.push(`${pct(Math.max(0, a - 0.001))} { opacity: 0; }`, `${pct(a + fade)} { opacity: 1; }`);
    if (b < T) frames.push(`${pct(b)} { opacity: 1; }`, `${pct(Math.min(T, b + fade))} { opacity: 0; }`);
  }
  if (windows[windows.length - 1][1] >= T) frames.push('100% { opacity: 1; }');
  else frames.push('100% { opacity: 0; }');
  return `@keyframes ${name} { ${frames.join(' ')} }`;
}

// The times of the story, in seconds.
const START = 1.2; // first press of F5
const STOP = 5.2; // second press of F5
const PASTE = 6.6; // the text is at the cursor
const HIDE = 7.9; // the pop-up hides
const RESET = 11.3; // the note goes back to the start

function pill({ cx, cy, width, height = 50, body, cls }) {
  const x = r2(cx - width / 2);
  const y = r2(cy - height / 2);
  return `<g class="${cls}"><rect x="${x}" y="${y}" width="${width}" height="${height}" rx="${height / 2}" fill="#16201B" fill-opacity="0.94" filter="url(#pill-shadow)"/>`
    + `<rect x="${x + 0.5}" y="${y + 0.5}" width="${width - 1}" height="${height - 1}" rx="${(height - 1) / 2}" fill="none" stroke="#fff" stroke-opacity="0.12"/>${body(x, y)}</g>`;
}

function glyphAt({ x, y, size, color, id, cls = '' }) {
  return `<g class="${cls}" transform="translate(${r2(x)} ${r2(y)}) scale(${r2(size / 32)})">${glyphInner({ id, color })}</g>`;
}

export function demoSvg() {
  const W = 960;
  const H = 560;
  const cx = W / 2;
  const pillY = 474;
  // Bars of the level meter: heights that look like speech, each with its own speed.
  const bars = Array.from({ length: 14 }, (_, i) => {
    const x = r2(cx - 64 + i * 9);
    const dur = r2(0.42 + ((i * 37) % 11) / 18);
    return `<rect class="bar" x="${x}" y="${pillY - 13}" width="4.5" height="26" rx="2.25" style="animation-duration:${dur}s;animation-delay:${r2(-i * 0.13)}s"/>`;
  }).join('');
  const times = [0, 1, 2, 3, 4].map((s) => `<text class="tick t${s}" x="${cx + 74}" y="${pillY + 5.5}">0:0${s}</text>`).join('');
  const recording = pill({
    cx, cy: pillY, width: 252, cls: 'rec',
    body: () => glyphAt({ x: cx - 112, y: pillY - 15, size: 30, color: '#FF453A', id: 'pill-rec', cls: 'spin-slow' }) + bars + times,
  });
  const transcribing = pill({
    cx, cy: pillY, width: 186, cls: 'busy',
    body: () => glyphAt({ x: cx - 75, y: pillY - 15, size: 30, color: '#F2F4FF', id: 'pill-busy', cls: 'spin-fast' })
      + `<text class="label" x="${cx - 33}" y="${pillY + 5.5}">Transcribing</text>`,
  });
  const done = pill({
    cx, cy: pillY, width: 132, cls: 'done',
    body: () => `<path d="M${cx - 42} ${pillY + 0.5} l6 6 l12.5 -13" fill="none" stroke="#32D74B" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"/>`
      + `<text class="label" x="${cx - 12}" y="${pillY + 5.5}">Pasted</text>`,
  });
  const key = (cls) => `<g class="key ${cls}"><rect x="806" y="438" width="74" height="66" rx="13" fill="#07120E" fill-opacity="0.4"/><g class="cap">`
    + `<rect x="806" y="432" width="74" height="66" rx="13" fill="url(#key)" stroke="#C9D2CC"/>`
    + `<text x="843" y="472" text-anchor="middle" class="keylabel">F5</text></g></g>`;
  const text = (y, s, cls = '') => `<text x="214" y="${y}" class="body ${cls}">${s}</text>`;

  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-label="Demo: push F5, speak, push F5 again. Moulin pastes the text in the note.">
<title>Moulin: push F5, speak, push F5 again, and the text is at the cursor</title>
<style>
  text { font-family: ${FONT}; }
  .menu { font-size: 14px; fill: #fff; }
  .menu.app { font-weight: 700; }
  .wtitle { font-size: 14px; font-weight: 600; fill: #3A3A40; }
  .date { font-size: 13px; fill: #8A8A93; }
  .body { font-size: 17.5px; fill: #1D1D22; }
  .caret { fill: ${C.green}; animation: blink 1.06s steps(1) infinite; }
  .label { font-size: 16px; font-weight: 600; fill: #fff; }
  .tick { font-size: 16px; font-weight: 600; fill: #fff; fill-opacity: 0.66; font-variant-numeric: tabular-nums; }
  .keylabel { font-size: 21px; font-weight: 600; fill: #1C1C22; }
  .bar { fill: #fff; fill-opacity: 0.9; transform-box: fill-box; transform-origin: center; animation: talk ease-in-out infinite alternate; }
  .spin-slow .sails, .spin-fast .sails, .menu-busy .sails { transform-box: fill-box; transform-origin: center; animation: turn linear infinite; }
  .spin-slow .sails { animation-duration: 2.6s; }
  .spin-fast .sails, .menu-busy .sails { animation-duration: 1.1s; }
  .rec, .busy, .done, .pasted, .flash, .caret0, .tick { animation-duration: ${T}s; animation-iteration-count: infinite; animation-timing-function: linear; }
  .rec { animation-name: rec; } .busy { animation-name: busy; } .done { animation-name: done; }
  .pills { animation: pills ${T}s infinite; transform-box: fill-box; transform-origin: center bottom; }
  .menu-idle { animation: menu-idle ${T}s infinite; } .menu-rec { animation: menu-rec ${T}s infinite; } .menu-busy { animation: menu-busy ${T}s infinite; }
  .key-a { animation: key-a ${T}s infinite; } .key-b { animation: key-b ${T}s infinite; }
  .key-a .cap, .key-b .cap { animation: press ${T}s infinite; }
  .key-b .cap { animation-name: press-b; }
  .pasted { animation-name: pasted; } .flash { animation-name: flash; }
  .caret0 { animation-name: caret0; }
  .t0 { animation-name: t0; } .t1 { animation-name: t1; } .t2 { animation-name: t2; } .t3 { animation-name: t3; } .t4 { animation-name: t4; }
  @keyframes turn { to { transform: rotate(-360deg); } }
  @keyframes talk { 0% { transform: scaleY(0.16); } 35% { transform: scaleY(0.7); } 60% { transform: scaleY(0.38); } 100% { transform: scaleY(0.95); } }
  @keyframes blink { 50% { fill-opacity: 0; } }
  ${shown('rec', [[START, STOP]])}
  ${shown('busy', [[STOP, PASTE]])}
  ${shown('done', [[PASTE, HIDE]])}
  @keyframes pills { 0%, ${pct(START - 0.01)} { transform: translateY(10px) scale(0.94); opacity: 0; } ${pct(START + 0.25)}, ${pct(HIDE)} { transform: none; opacity: 1; } ${pct(HIDE + 0.25)}, 100% { transform: translateY(10px) scale(0.94); opacity: 0; } }
  ${shown('menu-idle', [[0, START], [PASTE, T]], 0.01)}
  ${shown('menu-rec', [[START, STOP]], 0.01)}
  ${shown('menu-busy', [[STOP, PASTE]], 0.01)}
  ${shown('key-a', [[START - 0.35, START + 0.75]], 0.18)}
  ${shown('key-b', [[STOP - 0.35, STOP + 0.75]], 0.18)}
  @keyframes press { 0%, ${pct(START - 0.06)} { transform: none; } ${pct(START)}, ${pct(START + 0.16)} { transform: translateY(5px); } ${pct(START + 0.26)}, 100% { transform: none; } }
  @keyframes press-b { 0%, ${pct(STOP - 0.06)} { transform: none; } ${pct(STOP)}, ${pct(STOP + 0.16)} { transform: translateY(5px); } ${pct(STOP + 0.26)}, 100% { transform: none; } }
  ${shown('pasted', [[PASTE, RESET]], 0.08)}
  @keyframes flash { 0%, ${pct(PASTE - 0.01)} { opacity: 0; } ${pct(PASTE + 0.05)} { opacity: 1; } ${pct(PASTE + 1.1)}, 100% { opacity: 0; } }
  ${shown('caret0', [[0, PASTE], [RESET + 0.2, T]], 0.01)}
  ${[0, 1, 2, 3, 4].map((s) => shown(`t${s}`, [[START + s, s === 4 ? STOP : START + s + 1]], 0.01)).join('\n  ')}
  .rec, .busy, .menu-rec, .menu-busy, .key, .caret0, .flash, .tick { opacity: 0; }
  @media (prefers-reduced-motion: reduce) { * { animation: none !important; } }
</style>
<defs>
  <linearGradient id="key" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#FFFFFF"/><stop offset="1" stop-color="#ECEFEC"/></linearGradient>
  <clipPath id="screen"><rect width="${W}" height="${H}" rx="20"/></clipPath>
  <filter id="window-shadow" x="-20%" y="-20%" width="140%" height="150%"><feDropShadow dx="0" dy="22" stdDeviation="26" flood-color="#07120E" flood-opacity="0.4"/></filter>
  <filter id="pill-shadow" x="-30%" y="-60%" width="160%" height="260%"><feDropShadow dx="0" dy="8" stdDeviation="12" flood-color="#050B08" flood-opacity="0.45"/></filter>
</defs>
<g clip-path="url(#screen)">
  <rect width="${W}" height="${H}" fill="${C.green}"/>
  <path d="M0 ${H} L0 ${H - 46} C 300 ${H - 70}, 620 ${H - 78}, ${W} ${H - 56} L${W} ${H} Z" fill="${C.greenDeep}"/>

  <g class="menubar">
    <rect width="${W}" height="32" fill="#07120E" fill-opacity="0.2"/>
    <text x="24" y="21" class="menu app">Notes</text>
    <text x="84" y="21" class="menu">File</text><text x="124" y="21" class="menu">Edit</text><text x="164" y="21" class="menu">Format</text><text x="226" y="21" class="menu">View</text><text x="274" y="21" class="menu">Window</text>
    ${glyphAt({ x: 726, y: 6, size: 20, color: '#FFFFFF', id: 'menu-idle', cls: 'menu-idle' })}
    ${glyphAt({ x: 726, y: 6, size: 20, color: '#FF453A', id: 'menu-rec', cls: 'menu-rec' })}
    ${glyphAt({ x: 726, y: 6, size: 20, color: '#FFFFFF', id: 'menu-busy', cls: 'menu-busy' })}
    <path d="M771 13.5 a11 11 0 0 1 14 0 M773.6 16.4 a7 7 0 0 1 8.8 0 M776.2 19.2 a3 3 0 0 1 3.6 0" fill="none" stroke="#fff" stroke-width="1.8" stroke-linecap="round"/>
    <rect x="796" y="10.5" width="22" height="11" rx="3" fill="none" stroke="#fff" stroke-opacity="0.85" stroke-width="1.2"/><rect x="798" y="12.5" width="15" height="7" rx="1.6" fill="#fff"/><rect x="819.2" y="14" width="1.6" height="4" rx="0.8" fill="#fff" fill-opacity="0.85"/>
    <text x="936" y="21" class="menu" text-anchor="end">Wed 28 Oct 9:41</text>
  </g>

  <g filter="url(#window-shadow)">
    <rect x="178" y="70" width="604" height="300" rx="14" fill="#FFFFFF"/>
  </g>
  <rect x="178.5" y="70.5" width="603" height="299" rx="13.5" fill="none" stroke="#000" stroke-opacity="0.06"/>
  <circle cx="202" cy="93" r="6.5" fill="#FF5F57"/><circle cx="223" cy="93" r="6.5" fill="#FEBC2E"/><circle cx="244" cy="93" r="6.5" fill="#28C840"/>
  <text x="480" y="98" class="wtitle" text-anchor="middle">Standup</text>
  <path d="M178 116 H782" stroke="#000" stroke-opacity="0.07"/>
  <text x="214" y="150" class="date">Wednesday, October 28</text>
  ${text(186, 'Release 2.4 is on the cluster. The build took 12 minutes,')}
  ${text(213, 'and the CI failed twice on a load balancer timeout.')}
  <rect class="flash" x="204" y="234" width="556" height="66" rx="6" fill="#DCEAE2"/>
  <g class="pasted">
    ${text(258, 'The error rate went from 4.5% to 0.8%. Can you check the')}
    <text x="214" y="285" class="body">logs in Grafana before the 3 pm meeting?<tspan class="caret" dx="1">|</tspan></text>
  </g>
  <g class="caret0"><text x="212" y="258" class="body"><tspan class="caret">|</tspan></text></g>

  <g class="pills">${recording}${transcribing}${done}</g>
  ${key('key-a')}${key('key-b')}
</g>
</svg>`;
}
