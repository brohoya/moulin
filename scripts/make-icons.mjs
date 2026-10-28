// Makes the Moulin images from the shapes in docs/mill.js:
// - the app icon: build/icon.png, assets/icon.png, and assets/logo.svg
// - the tray images in assets/tray
// - the images of the website and the README in docs/media
// Run it with "npm run icons" after a change of the shapes or the colors.
import { Resvg } from '@resvg/resvg-js';
import fs from 'node:fs';
import path from 'node:path';
import { COLORS, glyphSvg, iconSvg } from '../docs/mill.js';
import { demoSvg, faviconSvg, heroSvg, socialSvg } from './media.mjs';

const ROOT = path.join(path.dirname(new URL(import.meta.url).pathname), '..');

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

const icon = iconSvg();
write('build/icon.png', png(icon, 1024));
write('assets/icon.png', png(icon, 256));
write('assets/logo.svg', icon);

// The sails have four-fold symmetry, so four frames of 22.5 degrees make a full turn.
const FRAMES = [45, 67.5, 90, 112.5];
// macOS menu bar: black template images, and colored images for the states that must stand out.
pair('assets/tray/mac-idleTemplate', glyphSvg(), 18);
FRAMES.forEach((angle, i) => pair(`assets/tray/mac-busy${i}Template`, glyphSvg({ angle }), 18));
pair('assets/tray/mac-recording', glyphSvg({ color: COLORS.signal }), 18);
pair('assets/tray/mac-error', glyphSvg({ color: COLORS.amber }), 18);
// Windows and Linux: a colored tile, readable on a light or a dark panel.
pair('assets/tray/tile-idle', glyphSvg({ color: COLORS.white, badge: COLORS.green }), 16);
FRAMES.forEach((angle, i) => pair(`assets/tray/tile-busy${i}`, glyphSvg({ angle, color: COLORS.white, badge: COLORS.green }), 16));
pair('assets/tray/tile-recording', glyphSvg({ color: COLORS.white, badge: COLORS.signal }), 16);
pair('assets/tray/tile-error', glyphSvg({ color: '#3A2600', badge: COLORS.amber }), 16);

// Website and README.
write('docs/media/icon.png', png(icon, 512));
write('docs/media/favicon.svg', faviconSvg());
write('docs/media/apple-touch-icon.png', png(faviconSvg({ square: true }), 180));
write('docs/media/banner.svg', heroSvg());
write('docs/media/demo.svg', demoSvg());
write('docs/media/social.png', png(socialSvg(), 1280));
console.log('Images written to build/, assets/, and docs/media/.');
