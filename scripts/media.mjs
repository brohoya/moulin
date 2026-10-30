// The images of the website and the README, made from the shapes in website/mill.js and website/wordmark.js.
// scripts/make-icons.mjs writes them to website/media and docs/media. The animations are CSS in the SVG files, so they also play in a README.
import { HILL, PAINT, SAIL_FINE, glyphInner, iconSvg, millSvg, windSvg } from '../website/mill.js';
import { WORDMARK } from '../website/wordmark.js';
import { DEFINITION } from './definition.mjs';

const r2 = (n) => Math.round(n * 100) / 100;
const FONT = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Ubuntu, 'Helvetica Neue', Arial, sans-serif";

function wordmark({ x, y, size, color, dot }) {
  const { d, dot: o } = WORDMARK;
  return `<g transform="translate(${x} ${y}) scale(${r2(size / 1000)})"><path d="${d}" fill="${color}"/><circle cx="${o.cx}" cy="${o.cy}" r="${o.r}" fill="${dot}"/></g>`;
}

// The definition under the wordmark, in two lines: "moulin à paroles [mulɛ̃ a paʀɔl]" and the meaning. size is the font size.
function definition({ x, y, size, color }) {
  const { phrase, ipa, meaning } = DEFINITION;
  const k = r2(size / 100);
  return `<g transform="translate(${x} ${y}) scale(${k})" fill="${color}">`
    + `<path transform="translate(0 ${-phrase.baseline})" d="${phrase.d}" fill-opacity="0.92"/>`
    + `<path transform="translate(${r2(phrase.width + 30)} ${-ipa.baseline})" d="${ipa.d}" fill-opacity="0.62"/>`
    + `<path transform="translate(0 ${150 - meaning.baseline})" d="${meaning.d}" fill-opacity="0.78"/></g>`;
}

// A wide banner in flat colors: the mill on a hill at the right, the wordmark and its definition at the left.
// Used for the README and the social card. hub is the x of the hub, ground is the top of the hill below the hub,
// and sail is the length of a sail, in pixels. A CSS animation turns the "sails" group around its origin, which is the hub.
export function heroSvg({ width = 1280, height = 560, hub = 920, ground = 526, sail = 200, word = { x: 96, y: 306, size: 180 }, defSize = 24, round = 28, animate = true } = {}) {
  const paint = PAINT;
  const scale = r2((sail / SAIL_FINE.length) * 1000) / 1000;
  const hubY = r2(ground - HILL * sail);
  const tower = r2((ground + 12 - hubY) / scale - 30); // The tower goes 12 px into the hill.
  const hill = `M0 ${height} L0 ${ground + 24} C ${r2(width * 0.3)} ${ground + 12}, ${hub - 300} ${ground - 2}, ${hub} ${ground} S ${width - 40} ${ground + 8}, ${width} ${ground + 14} L${width} ${height} Z`;
  // The far hill, behind the mill, in a paler green.
  const far = `M0 ${ground + 4} C ${r2(width * 0.2)} ${ground - 34}, ${r2(width * 0.42)} ${ground - 6}, ${r2(width * 0.6)} ${ground - 22} S ${r2(width * 0.9)} ${ground - 66}, ${width} ${ground - 44} L${width} ${height} L0 ${height} Z`;
  const style = animate ? `<style>
  .sails { animation: turn 32s linear infinite; }
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
  <rect width="${width}" height="${height}" fill="${paint.sky}"/>
  ${windSvg({ width, height, opacity: [0.95, 0.85, 0.75] })}
  <path d="${far}" fill="${paint.far}"/>
  <g transform="translate(${hub} ${hubY}) scale(${scale})">${millSvg({ height: tower, g: SAIL_FINE })}</g>
  <path d="${hill}" fill="${paint.land}"/>
  ${wordmark({ ...word, color: paint.ink, dot: paint.hub })}
  ${definition({ x: r2(word.x + 0.034 * word.size), y: r2(word.y + defSize * 2.3), size: defSize, color: paint.ink })}
</g>
</svg>`;
}

// The social card for GitHub and for link previews (1280 x 640).
export const socialSvg = () => heroSvg({ width: 1280, height: 640, hub: 920, ground: 604, sail: 228, word: { x: 100, y: 344, size: 184 }, defSize: 26, round: 0, animate: false });

// The favicon: the icon without its margin. square: a full square for the iOS home screen, which cuts its own shape.
export function faviconSvg({ square = false } = {}) {
  return iconSvg({ flat: true, square });
}

// ---------- The demo: one dictation in a note, with the menu bar icon, the pop-up, and the key. ----------
// The demo has flat colors and no filters: in Chromium and WebKit, the blur of a drop shadow showed a dark shape with a
// hard edge under the window.

const T = 12; // seconds for one loop
const pct = (t) => `${r2((t / T) * 100)}%`;

// Keyframes that show an element between the times of each window, with a fade at each end.
// A window must be longer than the fade: two keyframes at the same time make the browser fade the element at the wrong time.
function shown(name, windows, fade = 0.15) {
  const frames = [`0% { opacity: 0; }`];
  for (const [a, b] of windows) {
    if (b - a <= fade) throw new Error(`The window ${a}-${b} of ${name} is not longer than the fade (${fade} s).`);
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

// The pop-up: one dark pill whose width follows the state, as in the app (src/overlay/overlay.css). Its two round ends
// slide and its middle stretches, so that no second pill shows behind it while the width changes.
const PILL = { height: 50, rec: 252, busy: 186, done: 132 };
const EASE = 'cubic-bezier(0.3, 0.8, 0.3, 1)'; // the easing of the pop-up in the app
const RESIZE = 0.28; // seconds for a change of width
const APPEAR = 0.24; // seconds for the new content of the pill to fade in

// Keyframes for one part of the pill. transform gives the transform of the part for the width of each state.
function resize(name, transform) {
  return `@keyframes ${name} { 0%, ${pct(STOP)} { transform: ${transform(PILL.rec)}; animation-timing-function: ${EASE}; } `
    + `${pct(STOP + RESIZE)}, ${pct(PASTE)} { transform: ${transform(PILL.busy)}; animation-timing-function: ${EASE}; } `
    + `${pct(PASTE + RESIZE)}, 100% { transform: ${transform(PILL.done)}; } }`;
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
  // One label for each second of the recording (STOP - START = 4 s): 0:00 to 0:03.
  const times = [0, 1, 2, 3].map((s) => `<text class="tick t${s}" x="${cx + 74}" y="${pillY + 5.5}">0:0${s}</text>`).join('');
  // The pill at its largest width. The round ends slide in and the middle gets narrower for the next states.
  const end = PILL.rec / 2 - PILL.height / 2; // from the center of the pill to the center of a round end
  const slide = (w) => r2((PILL.rec - w) / 2);
  const stretch = (w) => Math.round(((w - PILL.height) / (PILL.rec - PILL.height)) * 10000) / 10000;
  const pill = `<g fill="#17221D"><circle class="end-l" cx="${cx - end}" cy="${pillY}" r="${PILL.height / 2}"/>`
    + `<circle class="end-r" cx="${cx + end}" cy="${pillY}" r="${PILL.height / 2}"/>`
    + `<rect class="mid" x="${cx - end}" y="${pillY - PILL.height / 2}" width="${2 * end}" height="${PILL.height}"/></g>`;
  const recording = `<g class="c-rec">${glyphAt({ x: cx - 112, y: pillY - 15, size: 30, color: '#FF453A', id: 'pill-rec', cls: 'spin-slow' })}${bars}${times}</g>`;
  const transcribing = `<g class="c-busy">${glyphAt({ x: cx - 75, y: pillY - 15, size: 30, color: '#F2F4FF', id: 'pill-busy', cls: 'spin-fast' })}`
    + `<text class="label" x="${cx - 33}" y="${pillY + 5.5}">Transcribing</text></g>`;
  const done = `<g class="c-done"><path d="M${cx - 42} ${pillY + 0.5} l6 6 l12.5 -13" fill="none" stroke="#32D74B" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"/>`
    + `<text class="label" x="${cx - 12}" y="${pillY + 5.5}">Pasted</text></g>`;
  const P = PAINT;
  const key = (cls) => `<g class="key ${cls}"><rect x="806" y="438" width="74" height="66" rx="13" fill="${P.land}" fill-opacity="0.3"/><g class="cap">`
    + `<rect x="806" y="432" width="74" height="66" rx="13" fill="url(#key)" stroke="#C9D2CC"/>`
    + `<text x="843" y="472" text-anchor="middle" class="keylabel">F5</text></g></g>`;
  const text = (y, s, cls = '') => `<text x="214" y="${y}" class="body ${cls}">${s}</text>`;

  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-label="Demo: push F5, speak, push F5 again. Moulin pastes the text in the note.">
<title>Moulin: push F5, speak, push F5 again, and the text is at the cursor</title>
<style>
  text { font-family: ${FONT}; }
  .menu { font-size: 14px; fill: #1D1D1F; }
  .menu.app { font-weight: 700; }
  .wtitle { font-size: 14px; font-weight: 600; fill: #3A3A40; }
  .date { font-size: 13px; fill: #8A8A93; }
  .body { font-size: 17.5px; fill: #1D1D22; }
  .caret { fill: ${P.land}; animation: blink 1.06s steps(1) infinite; }
  .label { font-size: 16px; font-weight: 600; fill: #fff; }
  .tick { font-size: 16px; font-weight: 600; fill: #fff; fill-opacity: 0.66; font-variant-numeric: tabular-nums; }
  .keylabel { font-size: 21px; font-weight: 600; fill: #1C1C22; }
  .bar { fill: #fff; fill-opacity: 0.9; transform-box: fill-box; transform-origin: center; animation: talk ease-in-out infinite alternate; }
  /* The hub of a glyph is at (16, 12.2) in its grid. With transform-box: fill-box, Firefox turns the gap in the mask around another point. */
  .spin-slow .sails, .spin-fast .sails, .menu-busy .sails { transform-box: view-box; transform-origin: 16px 12.2px; animation: turn linear infinite; }
  .spin-slow .sails { animation-duration: 2.6s; }
  .spin-fast .sails, .menu-busy .sails { animation-duration: 1.1s; }
  .c-rec, .c-busy, .c-done, .pasted, .flash, .caret0, .tick { animation-duration: ${T}s; animation-iteration-count: infinite; animation-timing-function: linear; }
  .c-rec { animation-name: c-rec; } .c-busy { animation-name: c-busy; } .c-done { animation-name: c-done; }
  .pills { animation: pills ${T}s infinite; transform-box: view-box; transform-origin: ${cx}px ${pillY + PILL.height / 2}px; }
  .end-l, .end-r, .mid { animation-duration: ${T}s; animation-iteration-count: infinite; }
  .end-l { animation-name: end-l; transform: translateX(${slide(PILL.done)}px); }
  .end-r { animation-name: end-r; transform: translateX(${-slide(PILL.done)}px); }
  .mid { animation-name: mid; transform: scaleX(${stretch(PILL.done)}); transform-box: view-box; transform-origin: ${cx}px ${pillY}px; }
  .menu-idle { animation: menu-idle ${T}s infinite; } .menu-rec { animation: menu-rec ${T}s infinite; } .menu-busy { animation: menu-busy ${T}s infinite; }
  .key-a { animation: key-a ${T}s infinite; } .key-b { animation: key-b ${T}s infinite; }
  .key-a .cap, .key-b .cap { animation: press ${T}s infinite; }
  .key-b .cap { animation-name: press-b; }
  .pasted { animation-name: pasted; } .flash { animation-name: flash; }
  .caret0 { animation-name: caret0; }
  .t0 { animation-name: t0; } .t1 { animation-name: t1; } .t2 { animation-name: t2; } .t3 { animation-name: t3; }
  @keyframes turn { to { transform: rotate(-360deg); } }
  @keyframes talk { 0% { transform: scaleY(0.16); } 35% { transform: scaleY(0.7); } 60% { transform: scaleY(0.38); } 100% { transform: scaleY(0.95); } }
  @keyframes blink { 50% { fill-opacity: 0; } }
  @keyframes c-rec { 0%, ${pct(STOP)} { opacity: 1; } ${pct(STOP + 0.01)}, 100% { opacity: 0; } }
  @keyframes c-busy { 0%, ${pct(STOP)} { opacity: 0; } ${pct(STOP + APPEAR)}, ${pct(PASTE)} { opacity: 1; } ${pct(PASTE + 0.01)}, 100% { opacity: 0; } }
  @keyframes c-done { 0%, ${pct(PASTE)} { opacity: 0; } ${pct(PASTE + APPEAR)}, 100% { opacity: 1; } }
  ${resize('end-l', (w) => `translateX(${slide(w)}px)`)}
  ${resize('end-r', (w) => `translateX(${-slide(w)}px)`)}
  ${resize('mid', (w) => `scaleX(${stretch(w)})`)}
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
  ${[0, 1, 2, 3].map((s) => shown(`t${s}`, [[START + s, START + s + 1]], 0.01)).join('\n  ')}
  .c-rec, .c-busy, .menu-rec, .menu-busy, .key, .caret0, .flash, .tick { opacity: 0; }
  @media (prefers-reduced-motion: reduce) { * { animation: none !important; } }
</style>
<defs>
  <linearGradient id="key" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#FFFFFF"/><stop offset="1" stop-color="#ECEFEC"/></linearGradient>
  <clipPath id="screen"><rect width="${W}" height="${H}" rx="20"/></clipPath>
</defs>
<g clip-path="url(#screen)">
  <rect width="${W}" height="${H}" fill="${P.sky}"/>
  <path d="M0 ${H - 66} C 260 ${H - 104}, 560 ${H - 78}, ${W} ${H - 100} L${W} ${H} L0 ${H} Z" fill="${P.far}"/>
  <path d="M0 ${H} L0 ${H - 46} C 300 ${H - 70}, 620 ${H - 78}, ${W} ${H - 56} L${W} ${H} Z" fill="${P.land}"/>

  <g class="menubar">
    <rect width="${W}" height="32" fill="#FFFFFF" fill-opacity="0.5"/>
    <text x="24" y="21" class="menu app">Notes</text>
    <text x="84" y="21" class="menu">File</text><text x="124" y="21" class="menu">Edit</text><text x="164" y="21" class="menu">Format</text><text x="226" y="21" class="menu">View</text><text x="274" y="21" class="menu">Window</text>
    ${glyphAt({ x: 726, y: 6, size: 20, color: '#1D1D1F', id: 'menu-idle', cls: 'menu-idle' })}
    ${glyphAt({ x: 726, y: 6, size: 20, color: '#FF3B30', id: 'menu-rec', cls: 'menu-rec' })}
    ${glyphAt({ x: 726, y: 6, size: 20, color: '#1D1D1F', id: 'menu-busy', cls: 'menu-busy' })}
    <path d="M774.32 16.72 A5.2 5.2 0 0 1 781.68 16.72 M771.64 14.04 A9 9 0 0 1 784.36 14.04" fill="none" stroke="#1D1D1F" stroke-width="1.8" stroke-linecap="round"/><circle cx="778" cy="20.4" r="1.7" fill="#1D1D1F"/>
    <rect x="796" y="10.5" width="22" height="11" rx="3" fill="none" stroke="#1D1D1F" stroke-opacity="0.85" stroke-width="1.2"/><rect x="798" y="12.5" width="15" height="7" rx="1.6" fill="#1D1D1F"/><rect x="819.2" y="14" width="1.6" height="4" rx="0.8" fill="#1D1D1F" fill-opacity="0.85"/>
    <text x="936" y="21" class="menu" text-anchor="end">Wed 28 Oct 9:41</text>
  </g>

  <rect x="178" y="70" width="604" height="300" rx="14" fill="#FFFFFF"/>
  <rect x="178.5" y="70.5" width="603" height="299" rx="13.5" fill="none" stroke="${P.land}" stroke-opacity="0.14"/>
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

  <g class="pills">${pill}${recording}${transcribing}${done}</g>
  ${key('key-a')}${key('key-b')}
</g>
</svg>`;
}
