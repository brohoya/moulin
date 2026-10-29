//! Changes the sample rate: a low-pass filter (windowed sinc, Blackman window) below the new Nyquist frequency,
//! then a linear interpolation between the filtered samples.

pub struct Resampler {
    step: f64,
    taps: Vec<f32>,
    history: Vec<f32>,
    pos: f64,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
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

    /// Adds the input samples, and appends the output samples that the input makes available.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<i16>) {
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
