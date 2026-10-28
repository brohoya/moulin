#!/usr/bin/env node
'use strict';
// Runs the E2E test in the installed (or built) Moulin app, with its signed-in session.
// A running Moulin quits first, because the app runs as one instance. The script starts it again after the test.
const { execFileSync, spawn, spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const ROOT = path.join(__dirname, '..');
const OUT = path.join(ROOT, 'artifacts');
const APPS = {
  darwin: [
    path.join(os.homedir(), 'Applications/Moulin.app/Contents/MacOS/Moulin'),
    '/Applications/Moulin.app/Contents/MacOS/Moulin',
    path.join(ROOT, 'dist/mac-arm64/Moulin.app/Contents/MacOS/Moulin'),
  ],
  win32: [path.join(process.env.LOCALAPPDATA || '', 'Programs', 'moulin', 'Moulin.exe'), path.join(ROOT, 'dist/win-unpacked/Moulin.exe')],
  linux: [path.join(ROOT, 'dist/linux-unpacked/moulin')],
};
const binary = (APPS[process.platform] || []).find((file) => fs.existsSync(file));
if (!binary) {
  console.error('Moulin is not installed or built. Run "npm run dist" first.');
  process.exit(1);
}

const name = path.basename(binary, '.exe');
const bundle = binary.replace(/\/Contents\/MacOS\/.*$/, ''); // macOS: the Moulin.app folder.
const isRunning = () => {
  if (process.platform === 'win32') return execFileSync('tasklist', ['/FI', `IMAGENAME eq ${name}.exe`], { encoding: 'utf8' }).includes(`${name}.exe`);
  return spawnSync('pgrep', ['-x', name]).status === 0;
};
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// macOS: "open" starts Moulin as its own app. When the terminal starts the binary, macOS 26 counts the menu bar icon
// as an icon of the terminal app, and keeps it hidden if that app is not allowed in the menu bar (also after the test).
// "open" does not give the exit code of the app, so the result comes from the new report.
function runWithOpen(args) {
  const log = path.join(os.tmpdir(), `moulin-e2e-${process.pid}`);
  const startedAt = Date.now();
  // The app does not get the environment of the terminal. PATH lets it find ffmpeg and ffprobe.
  const open = spawnSync('open', ['-W', '-n', '--env', `PATH=${process.env.PATH}`, '--stdout', `${log}.out`, '--stderr', `${log}.err`, bundle, '--args', ...args], { stdio: 'inherit' });
  for (const [file, stream] of [[`${log}.out`, process.stdout], [`${log}.err`, process.stderr]]) {
    if (!fs.existsSync(file)) continue;
    stream.write(fs.readFileSync(file, 'utf8'));
    fs.rmSync(file);
  }
  if (open.status !== 0) return 1;
  const reports = fs.existsSync(OUT) ? fs.readdirSync(OUT).filter((f) => /^e2e-.*\.json$/.test(f)).map((f) => path.join(OUT, f)) : [];
  const report = reports.filter((f) => fs.statSync(f).mtimeMs >= startedAt).sort().pop();
  if (!report) {
    console.error('Moulin wrote no E2E report.');
    return 1;
  }
  return JSON.parse(fs.readFileSync(report, 'utf8')).pass ? 0 : 1;
}

(async () => {
  const wasRunning = isRunning();
  if (wasRunning) {
    if (process.platform === 'win32') spawnSync('taskkill', ['/IM', `${name}.exe`]);
    else spawnSync('pkill', ['-TERM', '-x', name]);
    for (let i = 0; i < 50 && isRunning(); i++) await sleep(200);
  }
  console.log(`E2E in ${binary}`);
  // Extra arguments go to the test, for example --hold (npm run e2e -- --hold).
  const args = ['--e2e', `--sample=${path.join(ROOT, 'fixtures', 'fr-sample.wav')}`, `--out=${OUT}`, ...process.argv.slice(2)];
  const status = process.platform === 'darwin' ? runWithOpen(args) : (spawnSync(binary, args, { stdio: 'inherit' }).status ?? 1);
  if (wasRunning) {
    if (process.platform === 'darwin') spawnSync('open', ['-a', bundle]);
    else spawn(binary, [], { detached: true, stdio: 'ignore' }).unref();
  }
  process.exit(status);
})();
