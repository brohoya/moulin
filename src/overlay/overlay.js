'use strict';
// The pop-up during a dictation. Moulin records the microphone and sends the loudness of the voice to this page.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const pill = document.getElementById('pill');
const label = document.getElementById('text');
const time = document.getElementById('time');
const bars = [...document.querySelectorAll('#bars i')];
const sails = [...document.querySelectorAll('.mill .sails')];
const reduced = window.matchMedia('(prefers-reduced-motion: reduce)');
let meter = null;

// Between two visible states, the pill goes from its old width to its new width.
listen('overlay:state', ({ payload: { state, text } }) => {
  if (state === 'recording') startMeter();
  else stopMeter();
  pill.removeAttribute('data-hover');
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

pill.addEventListener('click', () => invoke('overlay_clicked'));

// While Moulin records, it sends the position of the mouse pointer: x and y from the top-left corner of the window,
// or null when the pointer is not over the window. WebKit does not report that the pointer leaves a window that is
// never the key window, so the page cannot use :hover.
listen('overlay:pointer', ({ payload }) => {
  const r = pill.getBoundingClientRect();
  const over = payload !== null && payload.x >= r.left && payload.x <= r.right && payload.y >= r.top && payload.y <= r.bottom;
  pill.toggleAttribute('data-hover', over);
});

// No menu of the web view (it has "Reload") on a right-click.
window.addEventListener('contextmenu', (event) => event.preventDefault());

// About 14 times per second. level: the RMS of 70 ms of audio. ms: the time since the start of the recording.
listen('overlay:level', ({ payload: { level, ms } }) => {
  if (!meter) return;
  meter.levels.shift();
  meter.levels.push(Math.min(1, level * 7));
  // A hidden page (a locked screen, or the pop-up off) draws nothing. There, each move of the bars starts CSS
  // transitions that WebKit keeps: about 1 MB per second of recording. The next event after the page shows moves them.
  if (!document.hidden) bars.forEach((bar, i) => { bar.style.transform = `scaleY(${0.12 + meter.levels[i] * 0.88})`; });
  const s = Math.floor(ms / 1000);
  time.textContent = `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
});

function startMeter() {
  if (meter) return;
  meter = { levels: bars.map(() => 0), angle: 0, speed: 0, previous: 0, frame: 0 };
  time.textContent = '0:00';
  const frame = (now) => {
    if (!meter) return;
    // The voice is the wind: a louder voice turns the sails faster, counterclockwise. As on the website (site.js), the
    // speed moves a part of the way to its target at each frame: the sails speed up fast and slow down slowly.
    if (meter.previous && !reduced.matches) {
      const dt = Math.min(0.05, (now - meter.previous) / 1000);
      const target = 20 + meter.levels[meter.levels.length - 1] * 280;
      meter.speed += (target - meter.speed) * Math.min(1, dt * (target > meter.speed ? 8 : 0.9));
      meter.angle -= meter.speed * dt;
      sails.forEach((group) => { group.style.transform = `rotate(${meter.angle.toFixed(1)}deg)`; });
    }
    meter.previous = now;
    meter.frame = requestAnimationFrame(frame);
  };
  meter.frame = requestAnimationFrame(frame);
}

function stopMeter() {
  if (!meter) return;
  cancelAnimationFrame(meter.frame);
  meter = null;
  bars.forEach((bar) => { bar.style.transform = ''; });
  sails.forEach((group) => { group.style.transform = ''; });
}
