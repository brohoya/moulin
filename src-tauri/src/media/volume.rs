//! The volume of the output device of the system: the speakers or the headphones. In "lower" mode, Moulin lowers it
//! during the recording, so that the media keeps playing more quietly, and sets it back after the recording.
//!
//! Moulin changes the volume of the device, not the volume of the media apps: macOS has no volume for one app, and
//! Windows and Linux keep the volume of an app after a restart of the app. If Moulin stopped during a recording, the
//! music would stay quiet with no visible reason. The volume of the device shows in the volume icon of the system.
//!
//! A change by the user during the recording stays: Moulin sets the volume back only if it did not change.
pub use imp::{Lowered, lower, restore};

/// Windows: the volume of the default output device (Core Audio, IAudioEndpointVolume), from 0 to 1, like the slider of
/// the volume.
#[cfg(windows)]
mod imp {
    use windows::{
        Win32::{
            Media::Audio::{Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender},
            System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree},
        },
        core::{HSTRING, Result},
    };

    /// A difference of volume that a driver can make when it keeps a value.
    const CLOSE: f32 = 0.01;

    /// A volume that Moulin lowered: the ID of the device, and its volume before and after.
    pub struct Lowered {
        device: String,
        before: f32,
        after: f32,
    }

    impl Lowered {
        /// The volume before and after, in percent, for the log.
        pub fn percents(&self) -> (u32, u32) {
            ((self.before * 100.0).round() as u32, (self.after * 100.0).round() as u32)
        }
    }

    /// Lowers the volume of the default output device to `percent` of its volume. None: the volume is already low.
    pub fn lower(percent: u8) -> std::result::Result<Option<Lowered>, String> {
        let lowered = || -> Result<Option<Lowered>> {
            let (device, volume) = endpoint(None)?;
            let before = unsafe { volume.GetMasterVolumeLevelScalar()? };
            let after = before * f32::from(percent) / 100.0;
            if before - after < CLOSE {
                return Ok(None);
            }
            unsafe { volume.SetMasterVolumeLevelScalar(after, std::ptr::null())? };
            Ok(Some(Lowered { device, before, after }))
        };
        lowered().map_err(|err| err.to_string())
    }

    /// Sets the volume back on the same device, if it is still the low volume. Returns false if the user changed it.
    pub fn restore(lowered: &Lowered) -> std::result::Result<bool, String> {
        let restored = || -> Result<bool> {
            let (_, volume) = endpoint(Some(&lowered.device))?;
            if (unsafe { volume.GetMasterVolumeLevelScalar()? } - lowered.after).abs() > CLOSE {
                return Ok(false);
            }
            unsafe { volume.SetMasterVolumeLevelScalar(lowered.before, std::ptr::null())? };
            Ok(true)
        };
        restored().map_err(|err| err.to_string())
    }

    /// The volume control of an output device: the default device, or the device with this ID. Also returns the ID.
    fn endpoint(id: Option<&str>) -> Result<(String, IAudioEndpointVolume)> {
        unsafe {
            // The media thread joins the COM apartment of the process. A second call does nothing.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = match id {
                Some(id) => devices.GetDevice(&HSTRING::from(id))?,
                None => devices.GetDefaultAudioEndpoint(eRender, eConsole)?,
            };
            let raw = device.GetId()?;
            let id = raw.to_string().unwrap_or_default();
            CoTaskMemFree(Some(raw.0 as *const _));
            Ok((id, device.Activate(CLSCTX_ALL, None)?))
        }
    }
}

/// Linux: the volume of the default sink, with pactl (PulseAudio, and PipeWire with pipewire-pulse), or with wpctl
/// (PipeWire) when pactl is missing.
#[cfg(target_os = "linux")]
mod imp {
    use super::super::{Failure, output};

    /// The full volume of one channel for pactl.
    const FULL: f64 = 65536.0;
    /// A difference of 1 % of the full volume.
    const CLOSE: f64 = 0.01;

    /// A volume that Moulin lowered.
    pub enum Lowered {
        /// pactl: the sink, and the raw volume of each channel before and after.
        Pulse { sink: String, before: Vec<u32>, after: Vec<u32> },
        /// wpctl: the node, and the volume before and after (1.0 is 100 %).
        Wire { node: String, before: f64, after: f64 },
    }

    impl Lowered {
        /// The volume before and after, in percent, for the log.
        pub fn percents(&self) -> (u32, u32) {
            let percent = |volume: f64| (volume * 100.0).round() as u32;
            let pulse = |raws: &[u32]| percent(f64::from(raws.iter().copied().max().unwrap_or(0)) / FULL);
            match self {
                Self::Pulse { before, after, .. } => (pulse(before), pulse(after)),
                Self::Wire { before, after, .. } => (percent(*before), percent(*after)),
            }
        }
    }

    /// Lowers the volume of the default sink to `percent` of its volume. None: the volume is already low.
    pub fn lower(percent: u8) -> Result<Option<Lowered>, String> {
        let factor = f64::from(percent) / 100.0;
        match lower_pulse(factor) {
            Err(Failure::Missing) => lower_wire(factor).map_err(|err| match err {
                Failure::Missing => "install pactl (pulseaudio-utils) or wpctl (wireplumber)".to_string(),
                Failure::Failed(message) => message,
            }),
            other => other.map_err(|err| err.to_string()),
        }
    }

    /// Sets the volume back on the same sink, if it is still the low volume. Returns false if the user changed it.
    pub fn restore(lowered: &Lowered) -> Result<bool, String> {
        let restored = match lowered {
            Lowered::Pulse { sink, before, after } => {
                pulse_volume(sink).and_then(|now| if close(&now, after) { set_pulse(sink, before).map(|()| true) } else { Ok(false) })
            }
            Lowered::Wire { node, before, after } => wire_volume(node).and_then(|now| {
                if (now - after).abs() > CLOSE {
                    return Ok(false);
                }
                output("wpctl", &["set-volume", node, &format!("{before:.4}")]).map(|_| true)
            }),
        };
        restored.map_err(|err| err.to_string())
    }

    fn lower_pulse(factor: f64) -> Result<Option<Lowered>, Failure> {
        let sink = output("pactl", &["get-default-sink"])?.trim().to_string();
        let before = pulse_volume(&sink)?;
        let after: Vec<u32> = before.iter().map(|raw| (f64::from(*raw) * factor).round() as u32).collect();
        if close(&before, &after) {
            return Ok(None);
        }
        set_pulse(&sink, &after)?;
        Ok(Some(Lowered::Pulse { sink, before, after }))
    }

    /// The raw volume of each channel of a sink. pactl writes "Volume: front-left: 26214 /  40% / -23.88 dB, ...".
    fn pulse_volume(sink: &str) -> Result<Vec<u32>, Failure> {
        let text = output("pactl", &["get-sink-volume", sink])?;
        let line = text.lines().find_map(|line| line.trim().strip_prefix("Volume:"));
        let raws = line.and_then(|line| {
            line.split(',').map(|channel| channel.split_once(':')?.1.split('/').next()?.trim().parse().ok()).collect::<Option<Vec<u32>>>()
        });
        raws.filter(|raws| !raws.is_empty()).ok_or_else(|| Failure::Failed(format!("pactl: unknown volume: {}", text.trim())))
    }

    /// Sets the raw volume of each channel.
    fn set_pulse(sink: &str, raws: &[u32]) -> Result<(), Failure> {
        let raws: Vec<String> = raws.iter().map(u32::to_string).collect();
        let args: Vec<&str> = ["set-sink-volume", sink].into_iter().chain(raws.iter().map(String::as_str)).collect();
        output("pactl", &args).map(drop)
    }

    /// True if each channel has the same volume, with a difference of 1 % at most.
    fn close(a: &[u32], b: &[u32]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(a, b)| (f64::from(*a) - f64::from(*b)).abs() <= CLOSE * FULL)
    }

    fn lower_wire(factor: f64) -> Result<Option<Lowered>, Failure> {
        // The first line of "wpctl inspect": "id 46, type PipeWire:Interface:Node".
        let text = output("wpctl", &["inspect", "@DEFAULT_AUDIO_SINK@"])?;
        let node = text.lines().next().and_then(|line| line.strip_prefix("id ")?.split(',').next()).map(str::to_string);
        let node = node.ok_or_else(|| Failure::Failed(format!("wpctl: unknown sink: {}", text.trim())))?;
        let before = wire_volume(&node)?;
        let after = before * factor;
        if before - after < CLOSE {
            return Ok(None);
        }
        output("wpctl", &["set-volume", &node, &format!("{after:.4}")])?;
        Ok(Some(Lowered::Wire { node, before, after }))
    }

    /// The volume of a node. wpctl writes "Volume: 0.40", or "Volume: 0.40 [MUTED]".
    fn wire_volume(node: &str) -> Result<f64, Failure> {
        let text = output("wpctl", &["get-volume", node])?;
        let volume = text.split_whitespace().nth(1).and_then(|value| value.parse().ok());
        volume.ok_or_else(|| Failure::Failed(format!("wpctl: unknown volume: {}", text.trim())))
    }
}

/// macOS: the output volume of the system, from 0 to 100, with osascript. This command needs no permission. macOS has
/// no volume for some devices, for example a display.
#[cfg(target_os = "macos")]
mod imp {
    use super::super::output;

    /// A difference of volume that macOS can make when it keeps a value.
    const CLOSE: i32 = 2;

    /// A volume that Moulin lowered, from 0 to 100.
    pub struct Lowered {
        before: u8,
        after: u8,
    }

    impl Lowered {
        /// The volume before and after, in percent, for the log.
        pub fn percents(&self) -> (u32, u32) {
            (self.before.into(), self.after.into())
        }
    }

    /// Lowers the output volume to `percent` of its volume. None: the volume is already low.
    pub fn lower(percent: u8) -> Result<Option<Lowered>, String> {
        let before = volume()?;
        let after = (u32::from(before) * u32::from(percent) / 100) as u8;
        if after >= before {
            return Ok(None);
        }
        set(after)?;
        Ok(Some(Lowered { before, after }))
    }

    /// Sets the volume back, if it is still the low volume. Returns false if the user changed it. osascript changes the
    /// default output device, so a new default device during the recording keeps its volume.
    pub fn restore(lowered: &Lowered) -> Result<bool, String> {
        if (i32::from(volume()?) - i32::from(lowered.after)).abs() > CLOSE {
            return Ok(false);
        }
        set(lowered.before).map(|()| true)
    }

    fn volume() -> Result<u8, String> {
        let text = output("/usr/bin/osascript", &["-e", "output volume of (get volume settings)"]).map_err(|err| err.to_string())?;
        text.trim().parse().map_err(|_| format!("the output device has no volume ({})", text.trim()))
    }

    fn set(volume: u8) -> Result<(), String> {
        output("/usr/bin/osascript", &["-e", &format!("set volume output volume {volume}")]).map(drop).map_err(|err| err.to_string())
    }
}
