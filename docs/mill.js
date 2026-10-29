// The Moulin mark: a windmill with four plain sails. In the app, the sails turn faster when the voice is louder.
// This file is a plain ES module with no dependencies. The website imports it in the browser.
// scripts/make-icons.mjs imports it in Node and makes the app icon, the tray images, and the README images.

// Flat colors, like a poster: a pale sky above a green land. The green is also the paint of the wood of the mill (the
// sails and the doors), as on windmill doors and on the Paris street furniture. The tower is cream stone, and the roof
// is sandstone. The red is only for the hub and the recording light.
export const COLORS = {
  sky: '#DCE5EA',
  far: '#C3D2CD', // The far hill.
  green: '#23493B',
  cream: '#F3EAD6',
  roof: '#D9C9A8',
  signal: '#FF3B30',
  amber: '#FF9F0A',
  white: '#FFFFFF',
};

// The color of each part of the picture. docs/site.css has the same colors.
export const PAINT = {
  sky: COLORS.sky, far: COLORS.far, land: COLORS.green, tower: COLORS.cream, roof: COLORS.roof, sails: COLORS.green,
  holes: COLORS.green, ring: COLORS.cream, hub: COLORS.signal, ink: COLORS.green,
};

// The loudness of the word "moulin" (macOS voice Thomas, 370 ms) in seven equal slices, in the order of the time.
// At the start, the website turns the sails with this loudness, as the app turns them with the voice.
export const VOICE = [0.73, 0.92, 1, 0.81, 0.78, 0.67, 0.44];

// The top of the hill is this number of sail lengths below the hub, in every picture of the mill: the website, the
// banners, and the app icon. The sails and the tower then keep the same proportions everywhere.
export const HILL = 1.42;

// One sail in the units of the 1024 icon: a spar from the hub, and a cloth on the trailing side that is wider at the
// tip than at the inner end (taper: the width at the inner end, as a part of the full width). The spar goes on for
// tip units past the cloth, as the whip of a real sail does. gap is the space between the spar and the cloth.
export const SAIL = { length: 330, inner: 70, width: 100, spar: 20, gap: 8, round: 5, tip: 10, taper: 0.62 };
// The same sail with a thinner spar, for large sizes (the website and the banners), like the display cut of a typeface.
export const SAIL_FINE = { ...SAIL, spar: 13, gap: 6, round: 3 };

// The length of the tower below the cap, so that the foot of the tower is HILL sail lengths below the hub.
const TOWER = Math.round(HILL * SAIL.length - 30);

const r2 = (n) => Math.round(n * 100) / 100;

// A rectangle with round corners, as a path.
function box(x, y, w, h, r) {
  return `M${r2(x + r)} ${r2(y)}h${r2(w - 2 * r)}a${r2(r)} ${r2(r)} 0 0 1 ${r2(r)} ${r2(r)}v${r2(h - 2 * r)}`
    + `a${r2(r)} ${r2(r)} 0 0 1 ${r2(-r)} ${r2(r)}h${r2(-(w - 2 * r))}a${r2(r)} ${r2(r)} 0 0 1 ${r2(-r)} ${r2(-r)}`
    + `v${r2(-(h - 2 * r))}a${r2(r)} ${r2(r)} 0 0 1 ${r2(r)} ${r2(-r)}Z`;
}

// The two parts of one sail that points up from the hub at (0, 0): the spar, and the cloth on the right side of the
// spar, so the sails turn counterclockwise, as most windmills in France do. The website opens the cloth at the start.
export function sailParts(g = SAIL) {
  const { length: L, inner, width: W, spar, gap, round: r, tip, taper } = g;
  const x = spar / 2 + gap;
  const low = r2(x + (W - x) * taper); // The free edge of the cloth at the inner end.
  return [
    { kind: 'spar', d: box(-spar / 2, -L - tip, spar, L + tip - spar, spar / 2) },
    { kind: 'cloth', d: `M${r2(x)} ${-inner} V${-L + r} Q${r2(x)} ${-L} ${r2(x + r)} ${-L} H${W - r} Q${W} ${-L} ${W} ${-L + r} L${low} ${-inner} Z` },
  ];
}

export const sailSvg = (g = SAIL) => sailParts(g).map(({ kind, d }) => `<path class="${kind}" d="${d}"/>`).join('');

// Four sails around the hub. angle is the angle of the first sail from the vertical, in degrees.
export const sailsSvg = (g = SAIL, angle = 45) =>
  [0, 90, 180, 270].map((a) => `<g class="sail" transform="rotate(${r2(angle + a)})">${sailSvg(g)}</g>`).join('');

// The tower, the cap, the door, and the window, around the hub at (0, 0). height is the length of the tower below the cap.
export function towerParts(height = TOWER) {
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

// The mill, around the hub at (0, 0). The green sails show well on the cream tower and the sandstone roof.
// The images use the colors of paint. The website gives its own colors to the classes.
export function millSvg({ height = TOWER, angle = 45, g = SAIL, paint = PAINT } = {}) {
  const t = towerParts(height);
  return `<path class="tower" d="${t.tower}" fill="${paint.tower}"/><path class="cap" d="${t.cap}" fill="${paint.roof}"/>`
    + `<path class="hole" d="${t.door}" fill="${paint.holes}"/><path class="hole" d="${t.window}" fill="${paint.holes}"/>`
    + `<g class="sails sails-face" fill="${paint.sails}">${sailsSvg(g, angle)}</g>`
    + `<circle class="ring" r="32" fill="${paint.ring}"/><circle class="hub" r="20" fill="${paint.hub}"/>`;
}

// The app icon (1024 x 1024): the macOS grid puts an 824 tile in the middle, with room for the shadow.
// flat: no shadow and no margin, for a favicon. square: a full square, for platforms that cut their own shape.
export function iconSvg({ angle = 45, flat = false, square = false } = {}) {
  const P = PAINT;
  const tile = square ? 'M100 100 H924 V924 H100 Z' : squirclePath(100, 100, 824, 185.4);
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${flat ? '100 100 824 824' : '0 0 1024 1024'}">
<defs>
  <clipPath id="tile"><path d="${tile}"/></clipPath>
  <filter id="drop" x="-10%" y="-10%" width="120%" height="130%"><feDropShadow dx="0" dy="12" stdDeviation="18" flood-color="#000" flood-opacity="0.28"/></filter>
</defs>
<path d="${tile}" fill="${P.sky}"${flat ? '' : ' filter="url(#drop)"'}/>
<g clip-path="url(#tile)">
  <path d="M60 846 C 260 790, 420 830, 600 808 S 860 770, 964 786 L964 1000 L60 1000 Z" fill="${P.far}"/>
  <path d="M60 902 C 300 852, 724 852, 964 902 L964 1000 L60 1000 Z" fill="${P.land}"/>
  <g transform="translate(512 440)">${millSvg({ angle })}</g>
</g>
</svg>`;
}

// The small mill for the tray and the pop-up, in a 32 x 32 grid. The hub is at (16, 12.2).
// Both "sails" groups (the visible sails and the gap that they cut in the tower) must turn together.
export function glyphInner({ id = 'glyph', angle = 45, color = '#000' } = {}) {
  const spar = '<rect x="-1" y="-11.4" width="2" height="10.4" rx="1"/>';
  // The cloth is wider at the tip, as on the large mill. At this size, it touches the spar.
  const sail = `${spar}<path d="M0 -4.2 V-11.4 H5.4 L3.6 -4.2 Z"/>`;
  const turn = (shape) => [0, 90, 180, 270].map((a) => `<g transform="translate(16 12.2) rotate(${r2(angle + a)})">${shape}</g>`).join('');
  const sails = turn(sail);
  return `<defs><mask id="${id}-gap" maskUnits="userSpaceOnUse" x="0" y="0" width="32" height="32"><rect width="32" height="32" fill="#fff"/>`
    + `<g class="sails" fill="#000" stroke="#000" stroke-width="2.4" stroke-linejoin="round">${sails}</g>`
    // No gap near the hub: the turning gaps would cut small, still pieces out of the cap and the top of the tower.
    + '<circle cx="16" cy="12.2" r="4.6" fill="#fff"/></mask></defs>'
    + `<g fill="${color}"><g mask="url(#${id}-gap)">`
    // No cap at this size: its two sides showed as still dots next to the hub while the sails turned.
    + '<path fill-rule="evenodd" d="M14.1 13.6 H17.9 L21.3 29.6 H10.7 Z M14.75 29.6 V26.4 A1.25 1.25 0 0 1 17.25 26.4 V29.6 Z"/></g>'
    + `<g class="sails">${sails}</g><circle cx="16" cy="12.2" r="1.9"/></g>`;
}

// The glyph as an SVG file. With badge, it sits on a colored tile (Windows and Linux trays).
export function glyphSvg({ angle = 45, color = '#000', badge = null } = {}) {
  const back = badge ? `<rect width="32" height="32" rx="7" fill="${badge}"/>` : '';
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">${back}${glyphInner({ angle, color })}</svg>`;
}

// ---------- The wind around the mill: used by the banners and by the website. ----------

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
