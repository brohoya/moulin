// Makes the Moulin icons from SVG: build/icon.png (app icon), assets/logo.svg, and the tray images in assets/tray.
// Run it with "npm run icons" after a change of the shapes or the colors.
import { Resvg } from '@resvg/resvg-js';
import fs from 'node:fs';
import path from 'node:path';

const ROOT = path.join(path.dirname(new URL(import.meta.url).pathname), '..');
const CREAM = '#FFF3E6';
const RED_TOP = '#FF6B6B';
const RED_BOTTOM = '#C9184A';
const RED_MID = '#E3324F';
const GOLD = '#FFC94A';

// One sail of the app icon: a spar and five bars of different lengths, like a sound wave. It points up from the hub.
const BARS = [60, 104, 140, 112, 76];
const bigSail = [
  '<rect x="-9" y="-318" width="18" height="300" rx="9"/>',
  ...BARS.map((w, i) => `<rect x="${-w / 2}" y="${-108 - i * 46}" width="${w}" height="30" rx="15"/>`),
].join('');

export function appIconSvg() {
  const sails = [45, 135, 225, 315].map((a) => `<g transform="translate(512 430) rotate(${a})">${bigSail}</g>`).join('');
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${RED_TOP}"/><stop offset="1" stop-color="${RED_BOTTOM}"/></linearGradient>
    <clipPath id="tile"><rect x="100" y="100" width="824" height="824" rx="186"/></clipPath>
    <filter id="shadow" x="-10%" y="-10%" width="120%" height="130%"><feDropShadow dx="0" dy="14" stdDeviation="16" flood-color="#000" flood-opacity="0.25"/></filter>
  </defs>
  <rect x="100" y="100" width="824" height="824" rx="186" fill="url(#bg)" filter="url(#shadow)"/>
  <g clip-path="url(#tile)">
    <path d="M60 880 Q512 760 964 880 L964 980 L60 980 Z" fill="#FFFFFF" fill-opacity="0.16"/>
    <path d="M452 474 L572 474 L628 830 L396 830 Z" fill="${CREAM}"/>
    <path d="M484 830 V762 A28 28 0 0 1 540 762 V830 Z" fill="${RED_BOTTOM}"/>
    <circle cx="512" cy="640" r="20" fill="${RED_BOTTOM}"/>
    <path d="M430 486 Q512 376 594 486 Z" fill="${CREAM}"/>
    <g fill="${CREAM}" stroke="${RED_MID}" stroke-width="14" paint-order="stroke">${sails}</g>
    <circle cx="512" cy="430" r="38" fill="${GOLD}" stroke="${RED_MID}" stroke-width="14"/>
  </g>
</svg>`;
}

// The small windmill for the tray and the pop-up (viewBox 0 0 32 32). The sails turn with "angle".
export function glyphSvg({ angle = 45, color = '#000', badge = null } = {}) {
  const sail = '<rect x="-1.1" y="-11.6" width="2.2" height="10.2" rx="1.1"/><rect x="-3" y="-11.6" width="6" height="7.4" rx="1.4"/>';
  const sails = [0, 90, 180, 270].map((a) => `<g transform="translate(16 12.5) rotate(${angle + a})">${sail}</g>`).join('');
  const back = badge
    ? `<rect x="0" y="0" width="32" height="32" rx="7" fill="${badge}"/>`
    : '';
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">
  <defs><mask id="gap"><rect width="32" height="32" fill="#fff"/><g fill="#000" stroke="#000" stroke-width="2.4">${sails}</g></mask></defs>
  ${back}
  <g fill="${color}">
    <g mask="url(#gap)">
      <path fill-rule="evenodd" d="M13.4 14.6 H18.6 L21.4 29.5 H10.6 Z M14.7 29.5 V26.2 A1.3 1.3 0 0 1 17.3 26.2 V29.5 Z"/>
      <path d="M12.3 15.4 Q16 10.4 19.7 15.4 Z"/>
    </g>
    ${sails}
    <circle cx="16" cy="12.5" r="1.9"/>
  </g>
</svg>`;
}

function png(svg, size) {
  return new Resvg(svg, { fitTo: { mode: 'width', value: size } }).render().asPng();
}

function write(rel, data) {
  const file = path.join(ROOT, rel);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, data);
}

// Writes name.png (size) and name@2x.png (2 x size).
function pair(rel, svg, size) {
  write(`${rel}.png`, png(svg, size));
  write(`${rel}@2x.png`, png(svg, size * 2));
}

write('build/icon.png', png(appIconSvg(), 1024));
write('assets/icon.png', png(appIconSvg(), 256));
write('assets/logo.svg', appIconSvg());
const FRAMES = [45, 67.5, 90, 112.5];
// macOS menu bar: black template images, and colored images for the states that must stand out.
pair('assets/tray/mac-idleTemplate', glyphSvg(), 18);
FRAMES.forEach((angle, i) => pair(`assets/tray/mac-busy${i}Template`, glyphSvg({ angle }), 18));
pair('assets/tray/mac-recording', glyphSvg({ color: '#FF3B30' }), 18);
pair('assets/tray/mac-error', glyphSvg({ color: '#FF9F0A' }), 18);
// Windows and Linux: a colored tile, readable on a light or a dark panel.
pair('assets/tray/tile-idle', glyphSvg({ color: CREAM, badge: RED_MID }), 16);
FRAMES.forEach((angle, i) => pair(`assets/tray/tile-busy${i}`, glyphSvg({ angle, color: CREAM, badge: RED_MID }), 16));
pair('assets/tray/tile-recording', glyphSvg({ color: '#FFFFFF', badge: '#FF3B30' }), 16);
pair('assets/tray/tile-error', glyphSvg({ color: '#3A2600', badge: '#FF9F0A' }), 16);
console.log('Icons written to build/ and assets/.');
