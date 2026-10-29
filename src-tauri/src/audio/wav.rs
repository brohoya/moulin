//! WAV files with 16-bit PCM samples: the recording that Moulin uploads, and the sample of the E2E test.
use std::path::Path;

/// A WAV file: PCM, mono, 16-bit.
pub fn encode(samples: &[i16], rate: u32) -> Vec<u8> {
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
pub fn read(file: &Path) -> Result<(u32, Vec<f32>), String> {
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
