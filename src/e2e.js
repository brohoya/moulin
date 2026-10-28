'use strict';
// E2E test: one full dictation in the real app, with the sample WAV as the microphone. It never pastes.
// It writes e2e-<time>.json and the recording e2e-<time>.webm to the output folder. Run it with "npm run e2e".
const { app, clipboard } = require('electron');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');
const fs = require('node:fs');
const path = require('node:path');

const exec = promisify(execFile);
const TAIL_S = 2.5; // The recording continues after the sample. With %noloop, this tail is silent.
const MAX_WER = 0.25;

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function next(events, name, ms, failOn) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`No "${name}" event after ${ms} ms`)), ms);
    events.once(name, (value) => { clearTimeout(timer); resolve(value); });
    if (failOn) events.once(failOn, (message) => { clearTimeout(timer); reject(new Error(`${failOn}: ${message}`)); });
  });
}

// Words for the word error rate: lower case, no punctuation. "j'ai" gives "j" and "ai". "2.4" and "4,5" stay one word.
function words(text) {
  return text.normalize('NFKC').toLowerCase().match(/[\p{L}\p{N}]+(?:[.,-][\p{L}\p{N}]+)*/gu) ?? [];
}

function wordErrorRate(reference, transcript) {
  const ref = words(reference);
  const hyp = words(transcript);
  let row = Array.from({ length: hyp.length + 1 }, (_, j) => j);
  for (let i = 1; i <= ref.length; i++) {
    const cur = [i];
    for (let j = 1; j <= hyp.length; j++) cur[j] = Math.min(row[j] + 1, cur[j - 1] + 1, row[j - 1] + (ref[i - 1] === hyp[j - 1] ? 0 : 1));
    row = cur;
  }
  return { wer: Number((row[hyp.length] / ref.length).toFixed(4)), edits: row[hyp.length], referenceWords: ref.length, transcriptWords: hyp.length };
}

async function seconds(file) {
  const { stdout } = await exec('ffprobe', ['-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', file]);
  return Number(stdout.trim());
}

// The loudest level of the recording between two times, in dB. Digital silence gives -91 dB or -inf.
async function maxVolumeDb(file, fromS, toS) {
  const trim = `atrim=start=${fromS}${toS ? `:end=${toS}` : ''}`;
  const { stderr } = await exec('ffmpeg', ['-hide_banner', '-nostats', '-i', file, '-af', `${trim},volumedetect`, '-f', 'null', '-']);
  const match = stderr.match(/max_volume: (\S+) dB/);
  if (!match) return null;
  return match[1] === '-inf' ? -Infinity : Number(match[1]);
}

// The trigger: a second Moulin process with --toggle, the same path as a system shortcut. Returns the run time of that process.
async function toggleFromOutside() {
  const startedAt = Date.now();
  await exec(process.execPath, app.isPackaged ? ['--toggle'] : [app.getAppPath(), '--toggle'], { timeout: 30_000 });
  return Date.now() - startedAt;
}

async function run({ events, settings, chatgpt, page, hotkeyOk, sample, outDir, captureOverlay }) {
  const startedAt = new Date().toISOString();
  const stamp = startedAt.replace(/[:.]/g, '-');
  const audioFile = path.join(outDir, `e2e-${stamp}.webm`);
  const reference = fs.readFileSync(sample.replace(/\.wav$/, '.txt'), 'utf8').trim();
  const failures = [];
  const trayStates = [];
  const overlayStates = [];
  const pictures = {}; // A picture of the pop-up in each state, for a visual check of the theme.
  events.on('state', (s) => trayStates.push(s.state));
  events.on('overlay', (o) => overlayStates.push(o));
  events.on('overlay', async ({ view, visible }) => {
    if (!visible || !['recording', 'transcribing', 'done'].includes(view) || pictures[view]) return;
    pictures[view] = 'pending';
    await delay(view === 'recording' ? 1500 : 350); // After the change of width, and with sound in the bars.
    try {
      const file = path.join(outDir, `e2e-${stamp}-pill-${view}.png`);
      fs.writeFileSync(file, (await captureOverlay()).toPNG());
      pictures[view] = path.basename(file);
    } catch (err) {
      pictures[view] = `not taken: ${err.message}`;
    }
  });
  const report = {
    test: 'One dictation: start and stop with "Moulin --toggle", upload, clipboard',
    startedAt,
    app: { name: app.getName(), version: app.getVersion(), packaged: app.isPackaged, executable: process.execPath },
    versions: { electron: process.versions.electron, chrome: process.versions.chrome, os: `${process.platform} ${process.getSystemVersion()}` },
    strategy: 'B: the app records the audio, and the hidden chatgpt.com page uploads it with fetch from an isolated world',
    hotkey: { accelerator: settings.hotkey, registered: hotkeyOk },
    trigger: 'a second Moulin process with --toggle (the path of a system shortcut)',
    page,
  };
  try {
    fs.mkdirSync(outDir, { recursive: true });
    if (!hotkeyOk) failures.push(`The hotkey ${settings.hotkey} is not registered`);
    if (!page.signedIn) failures.push('Not signed in to ChatGPT: open Moulin and sign in');
    await delay(2000);
    report.memoryMB = { lightPage: chatgpt.memoryMB(), ...(await chatgpt.measureFullPage()) };
    const sampleS = await seconds(sample);
    report.sample = { file: path.basename(sample), seconds: sampleS };

    const started = next(events, 'started', 15_000, 'micError');
    report.toggleProcessMs = { start: await toggleFromOutside() };
    await started;
    await delay((sampleS + TAIL_S) * 1000);
    const audioReady = next(events, 'audio', 10_000);
    const resultReady = next(events, 'result', 300_000);
    report.toggleProcessMs.stop = await toggleFromOutside();
    const audio = await audioReady;
    fs.writeFileSync(audioFile, audio.audio);
    const result = await resultReady;
    const clipboardText = await clipboard.readText();

    const tailFromS = Number((sampleS + 0.5).toFixed(2));
    const speechDb = await maxVolumeDb(audioFile, 0, sampleS);
    const tailDb = await maxVolumeDb(audioFile, tailFromS);
    const db = (v) => (Number.isFinite(v) ? v : String(v));
    report.recording = {
      file: path.basename(audioFile), mimeType: audio.mimeType, bytes: audio.audio.length, seconds: Number((audio.durationMs / 1000).toFixed(2)),
      speechMaxVolumeDb: db(speechDb), tailFromS, tailMaxVolumeDb: db(tailDb), noloop: tailDb !== null && tailDb < -60,
    };
    if (!(speechDb > -30)) failures.push(`The recording has no speech (${speechDb} dB): the fake microphone did not play the WAV file`);
    report.request = { httpStatus: result.status, attempts: result.attempts, page: result.page ?? page.kind, cfMitigated: result.cfMitigated ?? null, error: result.ok ? null : result.message };
    report.stopToClipboardMs = result.ok ? result.clipboardAt - result.stopAt : null;
    report.transcript = result.text ?? null;
    report.reference = reference;
    report.wordErrorRate = result.ok ? wordErrorRate(reference, result.text) : null;
    report.clipboard = { matchesTranscript: result.ok && clipboardText === result.text };
    report.trayStates = trayStates;
    report.overlay = overlayStates;
    for (let i = 0; i < 30 && Object.values(pictures).includes('pending'); i++) await delay(100);
    report.pictures = pictures;
    const shown = (view) => overlayStates.some((o) => o.view === view && o.visible);
    if (settings.showOverlay && !['recording', 'transcribing', 'done'].every(shown)) failures.push('The pop-up did not show the recording, transcribing, and done states');

    if (!report.recording.noloop) failures.push(`The recording after ${tailFromS} s is not silent (${tailDb} dB): %noloop does not work`);
    if (!result.ok) failures.push(`Upload: ${result.message}`);
    if (result.ok && report.wordErrorRate.wer > MAX_WER) failures.push(`The word error rate ${report.wordErrorRate.wer} is above ${MAX_WER}`);
    if (result.ok && !report.clipboard.matchesTranscript) failures.push('The clipboard text is not the transcript');
  } catch (err) {
    failures.push(err.message);
  }
  report.failures = failures;
  report.pass = failures.length === 0;
  const file = path.join(outDir, `e2e-${stamp}.json`);
  fs.writeFileSync(file, `${JSON.stringify(report, null, 2)}\n`);
  console.log(`E2E ${report.pass ? 'PASS' : 'FAIL'}: ${file}`);
  for (const failure of failures) console.log(`- ${failure}`);
  app.exit(report.pass ? 0 : 1);
}

module.exports = { run, wordErrorRate };
