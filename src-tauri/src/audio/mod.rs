//! The recorder: the microphone at 16 kHz, mono, 16-bit, in a WAV file. During the recording, it reports the
//! loudness about 14 times per second, for the pop-up. In the E2E test, the sample WAV plays one time instead of
//! the microphone, then silence follows until the stop.
mod resample;
mod wav;

use cpal::{
    FromSample, Sample, SampleFormat, SizedSample,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use resample::Resampler;
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
const RATE: u32 = 16_000;
/// The pop-up gets the loudness of each part of this length.
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
    let (stream, sink) = match open_microphone(id, on_level) {
        Ok(opened) => opened,
        Err(message) => {
            let _ = ready.send(Err(message));
            return None;
        }
    };
    let _ = ready.send(Ok(()));
    let keep = matches!(stop.recv(), Ok(Stop::Keep));
    drop(stream);
    let sink = sink.lock().ok()?;
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
                // Mono: the mean of the channels of each frame.
                sink.push(data.chunks(channels).map(|frame| frame.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / frame.len() as f32));
            }
        },
        |err| eprintln!("Microphone error: {err}"),
        None,
    )
}

fn play_sample(file: &Path, on_level: LevelFn, ready: &mpsc::Sender<Result<(), String>>, stop: &mpsc::Receiver<Stop>) -> Option<Recording> {
    let (rate, samples) = match wav::read(file) {
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
        sink.push((sent..due).map(|i| samples.get(i).copied().unwrap_or(0.0)));
        sent = due;
    };
    keep.then(|| sink.finish())
}

/// Collects the audio: loudness for the pop-up, then resampling to 16 kHz.
struct Sink {
    /// The mono samples of the last push. The buffer stays, so that the audio callback does not allocate.
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

    /// Takes the next mono samples at the rate of the source.
    fn push(&mut self, samples: impl IntoIterator<Item = f32>) {
        self.mono.clear();
        self.mono.extend(samples);
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

    fn finish(&self) -> Recording {
        Recording { duration_ms: self.out.len() as u64 * 1000 / u64::from(RATE), wav: wav::encode(&self.out, RATE) }
    }
}
