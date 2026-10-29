// The recorder: the microphone at 16 kHz, mono, 16-bit, in a WAV file. During the recording, it reports the loudness
// about 14 times per second, for the pop-up. In the E2E test, the sample WAV plays one time instead of the microphone,
// then silence follows until the stop.
use cpal::{
    FromSample, Sample, SampleFormat, SizedSample,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};

/// The sample rate of the recording. Speech-to-text models use 16 kHz.
pub const RATE: u32 = 16_000;
const LEVEL_MS: u32 = 70;
/// A dictation stops after 5 minutes. The buffer gets this size at the start, so that it never grows in the audio callback.
const MAX_SECONDS: usize = 305;

pub struct Recording {
    pub wav: Vec<u8>,
    pub duration_ms: u64,
}

pub enum Source {
    /// A device ID of cpal, or empty for the default microphone.
    Microphone(String),
    /// A WAV file (16-bit PCM) that plays one time, then silence.
    Sample(PathBuf),
}

#[derive(Serialize)]
pub struct Microphone {
    pub id: String,
    pub label: String,
}

/// The input devices, for the list in the settings. Their IDs stay the same after a restart.
pub fn microphones() -> Vec<Microphone> {
    let Ok(devices) = cpal::default_host().input_devices() else { return Vec::new() };
    devices
        .filter_map(|device| {
            let id = device.id().ok()?.to_string();
            let label = device.description().map(|d| d.name().to_owned()).unwrap_or_else(|_| id.clone());
            Some(Microphone { id, label })
        })
        .collect()
}

enum Stop {
    Keep,
    Discard,
}

type LevelFn = Box<dyn Fn(f32, u64) + Send>;

/// A running recording. The audio thread owns the stream, because a stream cannot move to another thread on all systems.
pub struct Recorder {
    stop: mpsc::Sender<Stop>,
    done: mpsc::Receiver<Option<Recording>>,
}

impl Recorder {
    /// Opens the source and starts to record. Returns when the audio flows, or with the reason of the failure.
    /// `on_level` gets the loudness (RMS of 70 ms, from 0 to 1) and the time since the start in ms.
    pub fn start(source: Source, on_level: impl Fn(f32, u64) + Send + 'static) -> Result<Recorder, String> {
        let (stop_tx, stop_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let on_level: LevelFn = Box::new(on_level);
        thread::Builder::new()
            .name("moulin-recorder".into())
            .spawn(move || {
                let recording = match source {
                    Source::Microphone(id) => record_microphone(&id, on_level, &ready_tx, &stop_rx),
                    Source::Sample(file) => play_sample(&file, on_level, &ready_tx, &stop_rx),
                };
                let _ = done_tx.send(recording);
            })
            .map_err(|err| err.to_string())?;
        match ready_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(())) => Ok(Recorder { stop: stop_tx, done: done_rx }),
            Ok(Err(message)) => Err(message),
            Err(_) => {
                let _ = stop_tx.send(Stop::Discard);
                Err("the microphone did not start in 10 seconds".into())
            }
        }
    }

    /// Stops the recording and returns the WAV file. It waits for the end of the audio thread.
    pub fn stop(self) -> Option<Recording> {
        let _ = self.stop.send(Stop::Keep);
        self.done.recv().ok().flatten()
    }

    pub fn cancel(self) {
        let _ = self.stop.send(Stop::Discard);
    }
}

fn record_microphone(
    id: &str,
    on_level: LevelFn,
    ready: &mpsc::Sender<Result<(), String>>,
    stop: &mpsc::Receiver<Stop>,
) -> Option<Recording> {
    let opened = open_microphone(id, on_level);
    let (stream, sink) = match opened {
        Ok(opened) => opened,
        Err(message) => {
            let _ = ready.send(Err(message));
            return None;
        }
    };
    let _ = ready.send(Ok(()));
    let keep = matches!(stop.recv(), Ok(Stop::Keep));
    drop(stream);
    let mut sink = sink.lock().ok()?;
    keep.then(|| sink.finish())
}

fn open_microphone(id: &str, on_level: LevelFn) -> Result<(cpal::Stream, Arc<Mutex<Sink>>), String> {
    let host = cpal::default_host();
    // A missing device (for example unplugged headphones) falls back to the default microphone.
    let chosen = id.parse().ok().and_then(|id| host.device_by_id(&id));
    let device = chosen.or_else(|| host.default_input_device()).ok_or("no microphone found")?;
    let supported = device.default_input_config().map_err(|err| err.to_string())?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let sink = Arc::new(Mutex::new(Sink::new(config.sample_rate, on_level)));
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, sink.clone()),
        SampleFormat::F64 => build::<f64>(&device, config, sink.clone()),
        SampleFormat::I16 => build::<i16>(&device, config, sink.clone()),
        SampleFormat::I32 => build::<i32>(&device, config, sink.clone()),
        SampleFormat::U16 => build::<u16>(&device, config, sink.clone()),
        SampleFormat::I8 => build::<i8>(&device, config, sink.clone()),
        SampleFormat::U8 => build::<u8>(&device, config, sink.clone()),
        other => return Err(format!("the sample format {other} is not supported")),
    }
    .map_err(|err| err.to_string())?;
    stream.play().map_err(|err| err.to_string())?;
    Ok((stream, sink))
}

fn build<T>(device: &cpal::Device, config: cpal::StreamConfig, sink: Arc<Mutex<Sink>>) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + Sample,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels.max(1));
    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            if let Ok(mut sink) = sink.lock() {
                sink.mono.clear();
                for frame in data.chunks(channels) {
                    let sum: f32 = frame.iter().map(|s| s.to_sample::<f32>()).sum();
                    sink.mono.push(sum / frame.len() as f32);
                }
                sink.flush_mono();
            }
        },
        |err| eprintln!("Microphone error: {err}"),
        None,
    )
}

fn play_sample(file: &Path, on_level: LevelFn, ready: &mpsc::Sender<Result<(), String>>, stop: &mpsc::Receiver<Stop>) -> Option<Recording> {
    let (rate, samples) = match read_wav(file) {
        Ok(wav) => wav,
        Err(message) => {
            let _ = ready.send(Err(format!("{}: {message}", file.display())));
            return None;
        }
    };
    let mut sink = Sink::new(rate, on_level);
    let _ = ready.send(Ok(()));
    let started = Instant::now();
    let mut sent = 0;
    let keep = loop {
        match stop.recv_timeout(Duration::from_millis(10)) {
            Ok(Stop::Keep) => break true,
            Ok(Stop::Discard) | Err(RecvTimeoutError::Disconnected) => break false,
            Err(RecvTimeoutError::Timeout) => {}
        }
        // Give the audio of the time that passed, as a real microphone does.
        let due = (started.elapsed().as_secs_f64() * f64::from(rate)) as usize;
        sink.mono.clear();
        sink.mono.extend((sent..due).map(|i| samples.get(i).copied().unwrap_or(0.0)));
        sent = due;
        sink.flush_mono();
    };
    keep.then(|| sink.finish())
}

/// Collects the audio: loudness for the pop-up, then resampling to 16 kHz.
struct Sink {
    mono: Vec<f32>,
    resampler: Resampler,
    out: Vec<i16>,
    rate: u32,
    frames: u64,
    level_sum: f64,
    level_len: usize,
    level_every: usize,
    on_level: LevelFn,
}

impl Sink {
    fn new(rate: u32, on_level: LevelFn) -> Self {
        Self {
            mono: Vec::with_capacity(8192),
            resampler: Resampler::new(rate, RATE),
            out: Vec::with_capacity(RATE as usize * MAX_SECONDS),
            rate,
            frames: 0,
            level_sum: 0.0,
            level_len: 0,
            level_every: (rate * LEVEL_MS / 1000).max(1) as usize,
            on_level,
        }
    }

    fn flush_mono(&mut self) {
        for (i, &s) in self.mono.iter().enumerate() {
            self.level_sum += f64::from(s * s);
            self.level_len += 1;
            if self.level_len == self.level_every {
                let rms = (self.level_sum / self.level_len as f64).sqrt() as f32;
                let ms = (self.frames + i as u64 + 1) * 1000 / u64::from(self.rate);
                (self.on_level)(rms, ms);
                self.level_sum = 0.0;
                self.level_len = 0;
            }
        }
        self.frames += self.mono.len() as u64;
        self.resampler.push(&self.mono, &mut self.out);
    }

    fn finish(&mut self) -> Recording {
        Recording { duration_ms: self.out.len() as u64 * 1000 / u64::from(RATE), wav: wav(&self.out, RATE) }
    }
}

/// Changes the sample rate: a low-pass filter (windowed sinc, Blackman window) below the new Nyquist frequency,
/// then a linear interpolation between the filtered samples.
struct Resampler {
    step: f64,
    taps: Vec<f32>,
    history: Vec<f32>,
    pos: f64,
}

impl Resampler {
    fn new(from: u32, to: u32) -> Self {
        let step = f64::from(from) / f64::from(to);
        // The filter has more taps for a higher input rate, so that its transition band stays about 2.7 kHz wide.
        let count = ((32.0 * step).ceil() as usize).max(31) | 1;
        let m = (count / 2) as f64;
        // The cutoff is 90 % of the lower Nyquist frequency, in cycles per input sample.
        let cutoff = 0.45 * f64::from(from.min(to)) / f64::from(from);
        let mut taps: Vec<f64> = (0..count)
            .map(|i| {
                let t = i as f64 - m;
                let sinc =
                    if t == 0.0 { 1.0 } else { (std::f64::consts::TAU * cutoff * t).sin() / (std::f64::consts::PI * t) / (2.0 * cutoff) };
                let phase = std::f64::consts::TAU * i as f64 / (count - 1) as f64;
                sinc * (0.42 - 0.5 * phase.cos() + 0.08 * (2.0 * phase).cos())
            })
            .collect();
        let sum: f64 = taps.iter().sum();
        taps.iter_mut().for_each(|t| *t /= sum);
        Self { step, taps: taps.into_iter().map(|t| t as f32).collect(), history: Vec::with_capacity(8192), pos: m }
    }

    fn filtered(&self, center: usize) -> f32 {
        let start = center - self.taps.len() / 2;
        self.history[start..start + self.taps.len()].iter().zip(&self.taps).map(|(x, t)| x * t).sum()
    }

    fn push(&mut self, input: &[f32], out: &mut Vec<i16>) {
        self.history.extend_from_slice(input);
        let m = self.taps.len() / 2;
        while (self.pos as usize) + 1 + m < self.history.len() {
            let i = self.pos as usize;
            let frac = (self.pos - i as f64) as f32;
            let (a, b) = (self.filtered(i), self.filtered(i + 1));
            let value = a + (b - a) * frac;
            out.push((value.clamp(-1.0, 1.0) * 32767.0).round() as i16);
            self.pos += self.step;
        }
        // Keep only the input that the next output samples need.
        let used = (self.pos as usize).saturating_sub(m);
        if used > 0 {
            self.history.drain(..used);
            self.pos -= used as f64;
        }
    }
}

/// A WAV file: PCM, mono, 16-bit.
fn wav(samples: &[i16], rate: u32) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes()); // bytes per second
    out.extend_from_slice(&2u16.to_le_bytes()); // bytes per frame
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Reads a WAV file with 16-bit PCM samples. Returns the sample rate and the mono samples.
fn read_wav(file: &Path) -> Result<(u32, Vec<f32>), String> {
    let bytes = std::fs::read(file).map_err(|err| err.to_string())?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let (mut format, mut data) = (None, None);
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let size = u32_at(at + 4) as usize;
        let body = at + 8..(at + 8 + size).min(bytes.len());
        match &bytes[at..at + 4] {
            b"fmt " if size >= 16 => format = Some((u16_at(at + 8), u16_at(at + 10), u32_at(at + 12), u16_at(at + 22))),
            b"data" => data = Some(body),
            _ => {}
        }
        at += 8 + size + size % 2;
    }
    let (Some((kind, channels, rate, bits)), Some(data)) = (format, data) else { return Err("no fmt or data chunk".into()) };
    if kind != 1 || bits != 16 || channels == 0 {
        return Err("the sample must be 16-bit PCM".into());
    }
    let channels = usize::from(channels);
    let samples = bytes[data]
        .chunks_exact(2 * channels)
        .map(|frame| frame.as_chunks::<2>().0.iter().map(|s| f32::from(i16::from_le_bytes(*s)) / 32768.0).sum::<f32>() / channels as f32)
        .collect();
    Ok((rate, samples))
}
