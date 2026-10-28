// The Moulin mark: a windmill whose sails hold the sound of the word "moulin".
// This file is a plain ES module with no dependencies. The website imports it in the browser.
// scripts/make-icons.mjs imports it in Node and makes the app icon, the tray images, and the README images.

export const COLORS = {
  skyTop: '#4A68FF',
  sky: '#2643DC',
  skyBottom: '#1D33C2',
  ground: '#172BA8',
  nightTop: '#070B24',
  night: '#10164A',
  nightBottom: '#1A2263',
  white: '#FFFFFF',
  tower: '#E9EDFF',
  signal: '#FF3B30',
  amber: '#FF9F0A',
  ink: '#121634',
};

// The loudness of the word "moulin" (macOS voice Thomas, 370 ms) in seven equal slices, from the tip of a sail to the hub.
export const VOICE = [0.73, 0.92, 1, 0.81, 0.78, 0.67, 0.44];

// One sail in the units of the 1024 icon: a spar from the hub, a frame on the trailing side, and one bar for each voice value.
export const SAIL = { length: 330, inner: 84, spar: 20, rail: 12, pad: 7, bar: 22, width: 116 };
// The same sail with thinner lines, for large sizes (the website and the banners), like the display cut of a typeface.
export const SAIL_FINE = { length: 330, inner: 84, spar: 13, rail: 7, pad: 9, bar: 15, width: 116 };

const r2 = (n) => Math.round(n * 100) / 100;

// The rectangles of one sail that points up from the hub at (0, 0). The frame is on the right side of the spar,
// so the sails turn counterclockwise, as most windmills in France do.
export function sailParts(g = SAIL, voice = VOICE) {
  const { length, inner, spar, rail, pad, bar, width } = g;
  const parts = [
    { kind: 'spar', x: -spar / 2, y: -length, w: spar, h: length - spar, r: spar / 2 },
    { kind: 'rail', x: width - rail, y: -length, w: rail, h: length - inner, r: rail / 2 },
    { kind: 'rail', x: 0, y: -length, w: width, h: rail, r: rail / 2 },
    { kind: 'rail', x: 0, y: -inner - rail, w: width, h: rail, r: rail / 2 },
  ];
  voice.forEach((v, i) => parts.push({ kind: 'bar', ...barRect(g, voice.length, i, v) }));
  return parts;
}

// The bar for slice i at level v (0 to 1), centered between the spar and the outer rail. The website calls it on each frame.
export function barRect(g, count, i, v) {
  const { length, inner, spar, rail, pad, bar } = g;
  const room = g.width - rail - spar / 2 - 2 * pad;
  const pitch = (length - inner - 2 * rail) / count;
  const w = Math.max(bar, Math.min(1, v) * room);
  return { x: spar / 2 + pad + (room - w) / 2, y: -length + rail + i * pitch + (pitch - bar) / 2, w, h: bar, r: bar / 2 };
}

const rect = ({ kind, x, y, w, h, r }) => `<rect class="${kind}" x="${r2(x)}" y="${r2(y)}" width="${r2(w)}" height="${r2(h)}" rx="${r2(r)}"/>`;

export const sailSvg = (g = SAIL, voice = VOICE) => sailParts(g, voice).map(rect).join('');

// Four sails around the hub. angle is the angle of the first sail from the vertical, in degrees.
export const sailsSvg = (g = SAIL, voice = VOICE, angle = 45) =>
  [0, 90, 180, 270].map((a) => `<g class="sail" transform="rotate(${r2(angle + a)})">${sailSvg(g, voice)}</g>`).join('');

// The tower, the cap, the door, and the window, around the hub at (0, 0). height is the length of the tower below the cap.
export function towerParts(height = 430) {
  const slope = 52 / 430; // The half width grows by this much for each unit of height.
  const top = 30;
  const bottom = top + height;
  const half = 66 + slope * height;
  const mid = top + height * 0.55; // The window: a little below the middle of the tower.
  return {
    tower: `M-66 ${top} L66 ${top} L${r2(half)} ${bottom} L${r2(-half)} ${bottom} Z`,
    cap: 'M-86 54 C-86 -48 86 -48 86 54 Z',
    door: `M-26 ${bottom} V${bottom - 58} A26 26 0 0 1 26 ${bottom - 58} V${bottom} Z`,
    window: `M-11 ${r2(mid + 17)} V${r2(mid - 6)} A11 11 0 0 1 11 ${r2(mid - 6)} V${r2(mid + 17)} Z`,
  };
}

// A rounded square with continuous corners, like the shape of a macOS app icon (corner smoothing 0.6).
export function squirclePath(x, y, size, radius, smoothing = 0.6) {
  const p = (1 + smoothing) * radius;
  const arcMeasure = 90 * (1 - smoothing);
  const rad = (deg) => (deg * Math.PI) / 180;
  const arc = Math.sin(rad(arcMeasure / 2)) * radius * Math.SQRT2;
  const alpha = (90 - arcMeasure) / 2;
  const p3p4 = radius * Math.tan(rad(alpha / 2));
  const beta = 45 * smoothing;
  const c = p3p4 * Math.cos(rad(beta));
  const d = c * Math.tan(rad(beta));
  const b = (p - arc - c - d) / 3;
  const a = 2 * b;
  const [A, C, D, ARC, R] = [a, c, d, arc, radius].map(r2);
  return [
    `M${r2(x + size - p)} ${y}`,
    `c${A} 0 ${r2(a + b)} 0 ${r2(a + b + c)} ${D}`, `a${R} ${R} 0 0 1 ${ARC} ${ARC}`, `c${D} ${C} ${D} ${r2(b + c)} ${D} ${r2(a + b + c)}`,
    `L${x + size} ${r2(y + size - p)}`,
    `c0 ${A} 0 ${r2(a + b)} ${-D} ${r2(a + b + c)}`, `a${R} ${R} 0 0 1 ${-ARC} ${ARC}`, `c${-C} ${D} ${r2(-(b + c))} ${D} ${r2(-(a + b + c))} ${D}`,
    `L${r2(x + p)} ${y + size}`,
    `c${-A} 0 ${r2(-(a + b))} 0 ${r2(-(a + b + c))} ${-D}`, `a${R} ${R} 0 0 1 ${-ARC} ${-ARC}`, `c${-D} ${-C} ${-D} ${r2(-(b + c))} ${-D} ${r2(-(a + b + c))}`,
    `L${x} ${r2(y + p)}`,
    `c0 ${-A} 0 ${r2(-(a + b))} ${D} ${r2(-(a + b + c))}`, `a${R} ${R} 0 0 1 ${ARC} ${-ARC}`, `c${C} ${-D} ${r2(b + c)} ${-D} ${r2(a + b + c)} ${-D}`,
    'Z',
  ].join('');
}

// The mill in front of the sky, around the hub at (0, 0). The sails cut a thin gap in the tower, so that they stay readable.
// ids makes the ids of the mask unique when one page has more than one mill.
export function millSvg({ id = 'mill', height = 430, angle = 45, voice = VOICE, g = SAIL, gap = 14, colors = COLORS, sailColor = colors.white, hole = colors.sky } = {}) {
  const t = towerParts(height);
  const sails = sailsSvg(g, voice, angle);
  const span = g.length + 2 * gap + height + 200;
  return `<defs><mask id="${id}-gap" maskUnits="userSpaceOnUse" x="${-span}" y="${-span}" width="${2 * span}" height="${2 * span}">`
    + `<rect x="${-span}" y="${-span}" width="${2 * span}" height="${2 * span}" fill="#fff"/>`
    + `<g class="sails" fill="#000" stroke="#000" stroke-width="${2 * gap}" stroke-linejoin="round">${sails}</g></mask></defs>`
    + `<g mask="url(#${id}-gap)"><path class="tower" d="${t.tower}" fill="${colors.tower}"/><path class="cap" d="${t.cap}" fill="${colors.white}"/>`
    + `<path class="hole" d="${t.door}" fill="${hole}"/><path class="hole" d="${t.window}" fill="${hole}"/></g>`
    + `<g class="sails sails-face" fill="${sailColor}">${sails}</g>`
    + `<circle class="ring" r="32" fill="${colors.white}"/><circle class="hub" r="20" fill="${colors.signal}"/>`;
}

// The app icon (1024 x 1024): the macOS grid puts an 824 tile in the middle, with room for the shadow.
// flat: no shadow and no margin, for a favicon. square: a full square, for platforms that cut their own shape.
export function iconSvg({ angle = 45, flat = false, square = false } = {}) {
  const C = COLORS;
  const tile = square ? 'M100 100 H924 V924 H100 Z' : squirclePath(100, 100, 824, 185.4);
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${flat ? '100 100 824 824' : '0 0 1024 1024'}">
<defs>
  <linearGradient id="sky" x1="0" y1="100" x2="0" y2="924" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="${C.skyTop}"/><stop offset="0.6" stop-color="${C.sky}"/><stop offset="1" stop-color="${C.skyBottom}"/></linearGradient>
  <radialGradient id="glow" cx="512" cy="440" r="430" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#A9BAFF" stop-opacity="0.5"/><stop offset="0.55" stop-color="#A9BAFF" stop-opacity="0.12"/><stop offset="1" stop-color="#A9BAFF" stop-opacity="0"/></radialGradient>
  <linearGradient id="rim" x1="0" y1="100" x2="0" y2="924" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#fff" stop-opacity="0.35"/><stop offset="0.3" stop-color="#fff" stop-opacity="0"/></linearGradient>
  <clipPath id="tile"><path d="${tile}"/></clipPath>
  <filter id="drop" x="-10%" y="-10%" width="120%" height="130%"><feDropShadow dx="0" dy="12" stdDeviation="18" flood-color="#000A40" flood-opacity="0.3"/></filter>
  <filter id="lift" x="-30%" y="-30%" width="160%" height="160%"><feDropShadow dx="0" dy="10" stdDeviation="12" flood-color="#0A1A8C" flood-opacity="0.3"/></filter>
</defs>
<path d="${tile}" fill="url(#sky)"${flat ? '' : ' filter="url(#drop)"'}/>
<g clip-path="url(#tile)">
  <rect x="100" y="100" width="824" height="824" fill="url(#glow)"/>
  <path d="M60 902 C 300 852, 724 852, 964 902 L964 1000 L60 1000 Z" fill="${C.ground}"/>
  <g transform="translate(512 440)" filter="url(#lift)">${millSvg({ id: 'icon', angle })}</g>
</g>
<path d="${tile}" fill="none" stroke="url(#rim)" stroke-width="3"/>
</svg>`;
}

// The small mill for the tray and the pop-up, in a 32 x 32 grid. The hub is at (16, 12.2).
// Both "sails" groups (the visible sails and the gap that they cut in the tower) must turn together.
export function glyphInner({ id = 'glyph', angle = 45, color = '#000' } = {}) {
  const sail = '<rect x="-1" y="-11.4" width="2" height="10.4" rx="1"/>'
    + '<path fill-rule="evenodd" d="M0 -11.4 H5.4 V-4.2 H0 Z M1.2 -9.8 V-5.8 H3.8 V-9.8 Z"/>';
  const sails = [0, 90, 180, 270].map((a) => `<g transform="translate(16 12.2) rotate(${r2(angle + a)})">${sail}</g>`).join('');
  return `<defs><mask id="${id}-gap" maskUnits="userSpaceOnUse" x="0" y="0" width="32" height="32"><rect width="32" height="32" fill="#fff"/>`
    + `<g class="sails" fill="#000" stroke="#000" stroke-width="2.4" stroke-linejoin="round">${sails}</g></mask></defs>`
    + `<g fill="${color}"><g mask="url(#${id}-gap)">`
    + '<path fill-rule="evenodd" d="M13.5 14.2 H18.5 L21.3 29.6 H10.7 Z M14.75 29.6 V26.4 A1.25 1.25 0 0 1 17.25 26.4 V29.6 Z"/>'
    + '<path d="M12.4 15.2 C 12.4 10.6, 19.6 10.6, 19.6 15.2 Z"/></g>'
    + `<g class="sails">${sails}</g><circle cx="16" cy="12.2" r="1.9"/></g>`;
}

// The glyph as an SVG file. With badge, it sits on a colored tile (Windows and Linux trays).
export function glyphSvg({ angle = 45, color = '#000', badge = null } = {}) {
  const back = badge ? `<rect width="32" height="32" rx="7" fill="${badge}"/>` : '';
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">${back}${glyphInner({ angle, color })}</svg>`;
}

// ---------- The sky around the mill: used by the banners and by the website. ----------

// A seeded random generator, so that the stars stay at the same place after each run.
function random(seed) {
  let s = seed;
  return () => {
    s = (s * 16807) % 2147483647;
    return (s - 1) / 2147483646;
  };
}

// Thin streaks of wind that slide along long curves. The CSS class "wind" animates them with the variable --run.
export function windSvg({ width, height, opacity = [0.3, 0.26, 0.22] }) {
  const rows = [
    { y: 0.2, amp: 18, dash: 140, gap: 1100, dur: 9, delay: -2 },
    { y: 0.36, amp: -14, dash: 90, gap: 900, dur: 7, delay: -5 },
    { y: 0.78, amp: 12, dash: 170, gap: 1300, dur: 11, delay: -8 },
  ];
  return rows.map((w, i) => {
    const y = r2(height * w.y);
    const d = `M${-200} ${y} C ${r2(width * 0.25)} ${r2(y - w.amp)}, ${r2(width * 0.6)} ${r2(y + w.amp)}, ${width + 200} ${r2(y - w.amp / 2)}`;
    return `<path class="wind" d="${d}" stroke-dasharray="${w.dash} ${w.gap}" style="animation-duration:${w.dur}s;animation-delay:${w.delay}s;--run:${-(w.dash + w.gap)}px" stroke-opacity="${opacity[i]}"/>`;
  }).join('');
}

// Small stars for the night sky. A few of them have the CSS class "twinkle". No star falls in the boxes of avoid (for example, text).
export function starsSvg({ width, height, count = 46, seed = 7, avoid = [] }) {
  const rnd = random(seed);
  let out = '';
  for (let i = 0; i < count; i++) {
    const x = r2(rnd() * width);
    const y = r2(rnd() ** 1.6 * height * 0.78);
    const r = r2(0.6 + rnd() * 1.3);
    const o = r2(0.25 + rnd() * 0.65);
    const twinkle = i % 5 === 0 ? ` class="twinkle" style="animation-delay:${r2(-rnd() * 6)}s"` : '';
    if (avoid.some((b) => x > b.x && x < b.x + b.w && y > b.y && y < b.y + b.h)) continue;
    out += `<circle cx="${x}" cy="${y}" r="${r}" fill="#fff" fill-opacity="${o}"${twinkle}/>`;
  }
  return out;
}
