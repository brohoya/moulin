// Makes the Moulin images from the shapes in website/mill.js:
// - the app icon: src-tauri/icons/icon.svg, the bundle icons in src-tauri/icons, and src/settings/icon.png
// - the tray images in src-tauri/icons/tray
// - the images of the website in website/media, and the images of the README in docs/media
// Run it with "npm run icons" after a change of the shapes or the colors.
import { Resvg } from '@resvg/resvg-js';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { COLORS, glyphSvg, iconSvg } from '../website/mill.js';
import { demoSvg, faviconSvg, heroSvg, socialSvg } from './media.mjs';

const ROOT = path.join(path.dirname(new URL(import.meta.url).pathname), '..');
const ICONS = path.join(ROOT, 'src-tauri', 'icons');

function png(svg, size) {
  return new Resvg(svg, { fitTo: { mode: 'width', value: size } }).render().asPng();
}

function write(rel, data) {
  const file = path.join(ROOT, rel);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, data);
}

// A tray image has two times the size of the tray (18 pt on macOS, 16 px on Windows and Linux), so that it is sharp
// on a Retina screen. The system scales it.
function tray(name, svg, size) {
  write(`src-tauri/icons/tray/${name}.png`, png(svg, size * 2));
}

const icon = iconSvg();
write('src-tauri/icons/icon.svg', icon);
write('src/settings/icon.png', png(icon, 256));

// The sails have four-fold symmetry, so four frames of 22.5 degrees make a full turn.
const FRAMES = [45, 67.5, 90, 112.5];
// macOS menu bar: black template images, and colored images for the states that must stand out.
tray('mac-idle', glyphSvg(), 18);
FRAMES.forEach((angle, i) => tray(`mac-busy${i}`, glyphSvg({ angle }), 18));
tray('mac-recording', glyphSvg({ color: COLORS.signal }), 18);
tray('mac-error', glyphSvg({ color: COLORS.amber }), 18);
// Windows and Linux: a colored tile, readable on a light or a dark panel.
tray('tile-idle', glyphSvg({ color: COLORS.white, badge: COLORS.green }), 16);
FRAMES.forEach((angle, i) => tray(`tile-busy${i}`, glyphSvg({ angle, color: COLORS.white, badge: COLORS.green }), 16));
tray('tile-recording', glyphSvg({ color: COLORS.white, badge: COLORS.signal }), 16);
tray('tile-error', glyphSvg({ color: '#3A2600', badge: COLORS.amber }), 16);

// Website and README.
write('website/media/favicon.svg', faviconSvg());
write('website/media/apple-touch-icon.png', png(faviconSvg({ square: true }), 180));
write('docs/media/banner.svg', heroSvg());
write('docs/media/demo.svg', demoSvg());
write('website/media/social.png', png(socialSvg(), 1280));
// The bundle icons (icns, ico, and png), from icon.svg. "tauri icon" also makes icons for Android, iOS, and the
// Microsoft Store, which Moulin does not use, so the script removes them.
execFileSync('npx', ['tauri', 'icon', path.join(ICONS, 'icon.svg')], { cwd: ROOT, stdio: 'inherit' });
const KEEP = ['icon.svg', 'tray', '32x32.png', '128x128.png', '128x128@2x.png', 'icon.icns', 'icon.ico'];
for (const name of fs.readdirSync(ICONS)) {
  if (!KEEP.includes(name)) fs.rmSync(path.join(ICONS, name), { recursive: true });
}
console.log('Images written to src-tauri/icons/, src/settings/, website/media/, and docs/media/.');
