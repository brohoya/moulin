'use strict';
// The pop-up during a dictation. This page also records the microphone (webm/opus, mono) when the main process asks.
const api = window.moulin;
const pill = document.getElementById('pill');
const label = document.getElementById('text');
const time = document.getElementById('time');
const bars = [...document.querySelectorAll('#bars i')];
const sails = [...document.querySelectorAll('.mill .sails')];
const reduced = window.matchMedia('(prefers-reduced-motion: reduce)');
let recorder = null;
let starting = false;
let stopAsked = null; // 'stop' or 'cancel' when the command comes before the microphone is open.
let meter = null;

// Between two visible states, the pill goes from its old width to its new width.
api.onState(({ state, text }) => {
  const shown = pill.dataset.state !== 'hidden' && state !== 'hidden' && !reduced.matches;
  const from = pill.getBoundingClientRect().width;
  pill.style.width = '';
  pill.dataset.state = state;
  label.textContent = text || '';
  if (!shown) return;
  const to = pill.getBoundingClientRect().width;
  if (Math.abs(to - from) < 1) return;
  pill.style.width = `${from}px`;
  pill.getBoundingClientRect(); // Applies the old width before the transition starts.
  pill.style.width = `${to}px`;
});

pill.addEventListener('transitionend', (event) => {
  if (event.propertyName === 'width') pill.style.width = '';
});

pill.addEventListener('click', () => api.clicked());

// A missing device (for example unplugged headphones) falls back to the default microphone.
async function openMicrophone(deviceId) {
  const audio = { channelCount: 1 };
  if (deviceId) {
    try {
      return await navigator.mediaDevices.getUserMedia({ audio: { ...audio, deviceId: { exact: deviceId } } });
    } catch (err) {
      if (err.name !== 'OverconstrainedError' && err.name !== 'NotFoundError') throw err;
    }
  }
  return navigator.mediaDevices.getUserMedia({ audio });
}

function startMeter(stream, startedAt) {
  const context = new AudioContext();
  const analyser = context.createAnalyser();
  analyser.fftSize = 512;
  context.createMediaStreamSource(stream).connect(analyser);
  const samples = new Float32Array(analyser.fftSize);
  const levels = bars.map(() => 0);
  let last = 0;
  let previous = 0;
  let angle = 0;
  const frame = (now) => {
    if (!meter) return;
    if (now - last > 70) {
      last = now;
      analyser.getFloatTimeDomainData(samples);
      const rms = Math.sqrt(samples.reduce((sum, v) => sum + v * v, 0) / samples.length);
      levels.shift();
      levels.push(Math.min(1, rms * 7));
      bars.forEach((bar, i) => { bar.style.transform = `scaleY(${0.12 + levels[i] * 0.88})`; });
      const s = Math.floor((performance.now() - startedAt) / 1000);
      time.textContent = `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
    }
    // The voice is the wind: a louder voice turns the sails faster, counterclockwise.
    if (previous && !reduced.matches) {
      angle -= (20 + levels[levels.length - 1] * 240) * ((now - previous) / 1000);
      sails.forEach((group) => { group.style.transform = `rotate(${angle.toFixed(1)}deg)`; });
    }
    previous = now;
    meter.frame = requestAnimationFrame(frame);
  };
  time.textContent = '0:00';
  meter = { context, frame: requestAnimationFrame(frame) };
}

function stopMeter() {
  if (!meter) return;
  cancelAnimationFrame(meter.frame);
  meter.context.close();
  meter = null;
  bars.forEach((bar) => { bar.style.transform = ''; });
  sails.forEach((group) => { group.style.transform = ''; });
}

api.onCommand(async ({ command, deviceId }) => {
  if (command === 'stop' || command === 'cancel') {
    if (recorder?.state === 'recording') {
      recorder.cancelled = command === 'cancel';
      recorder.stop();
    } else if (starting) {
      stopAsked = command;
    }
    return;
  }
  if (command !== 'start' || recorder || starting) return;
  starting = true;
  stopAsked = null;
  try {
    const stream = await openMicrophone(deviceId);
    const chunks = [];
    const rec = new MediaRecorder(stream, { mimeType: 'audio/webm;codecs=opus', audioBitsPerSecond: 32000 });
    let startedAt = 0;
    rec.ondataavailable = (event) => chunks.push(event.data);
    rec.onstop = async () => {
      const durationMs = performance.now() - startedAt;
      stopMeter();
      stream.getTracks().forEach((track) => track.stop());
      recorder = null;
      if (rec.cancelled) return;
      const blob = new Blob(chunks, { type: rec.mimeType });
      api.audio(new Uint8Array(await blob.arrayBuffer()), rec.mimeType, durationMs);
    };
    rec.start();
    startedAt = performance.now();
    recorder = rec;
    startMeter(stream, startedAt);
    api.started();
    if (stopAsked) {
      rec.cancelled = stopAsked === 'cancel';
      rec.stop();
    }
  } catch (err) {
    api.error(err.message || String(err));
  } finally {
    starting = false;
  }
});
