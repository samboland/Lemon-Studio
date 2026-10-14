//! Settings > Audio: output device, sample rate and buffer length (they work), the status lines, CPU and mixer options.
// spec: ui/settings.md "Audio"

use super::{combo, g, Group, Row, SAct};
use crate::app::{App, State};

/// The checks and buttons of this page that work (`SAct::Check` / `SAct::Button` labels, handled in `act`).
pub const LIVE: &[&str] = &[];

/// The combos that work (Audio page: they open a menu of choices, `App::audio_menu`).
pub const LIVE_COMBOS: [&str; 3] = [OUTPUT_DEVICE, SAMPLE_RATE, BUFFER_LENGTH];
pub const OUTPUT_DEVICE: &str = "Output device";
pub const SAMPLE_RATE: &str = "Sample rate (Hz)";
pub const BUFFER_LENGTH: &str = "Buffer length";

/// A buffer length as the Audio page shows it: frames and milliseconds at `rate`.
pub fn buffer_text(frames: u32, rate: u32) -> String {
    format!("{frames} frames ({:.1} ms)", f64::from(frames) * 1000.0 / f64::from(rate.max(1)))
}

/// The Audio page's groups (spec §4).
pub fn page(s: &State) -> Vec<Group> {
    use Row::*;
    let st = &s.settings;
    // Device, rate and buffer length work (spec: ui/settings.md "Audio"); a choice of "the device's own" shows
    // what that is.
    let rate = st.rate.unwrap_or(44100);
    let rate_text = match st.audio.rate {
        Some(r) => r.to_string(),
        None => format!("Device ({rate})"),
    };
    let buffer = match st.audio.buffer {
        Some(b) => buffer_text(b, rate),
        None => "Device default".into(),
    };
    vec![
        g("Input / output", vec![
            combo(&st.device, 360.0, OUTPUT_DEVICE),
            combo(&rate_text, 120.0, SAMPLE_RATE),
            combo(&buffer, 160.0, BUFFER_LENGTH),
            Check("Auto close", false),
            Text(st.status[0].clone(), true),
            Text(st.status[1].clone(), true),
            List(vec![("Output devices", 0.0)], st.devices.iter().map(|d| vec![d.clone()]).collect(), 4),
            Check2("Mix in buffer switch", false, "Triple buffer", false),
            combo("Highest", 96.0, "Priority"),
            Check("Safe overloads", true),
            combo("Mixer", 96.0, "Playback tracking"),
        ]),
        g("CPU", vec![
            Check2("Multithreaded generator processing", true, "Smart disable", true),
            Check2("Multithreaded mixer processing", true, "Align tick lengths", false),
        ]),
        g("Mixer", vec![
            combo("24-point sinc", 122.0, "Resampling quality"),
            Value("---".into(), "Browser preview track"),
            Value("---".into(), "Metronome track"),
            Check("Play truncated notes on transport", true),
            Check("Reset plugins on transport", false),
        ]),
    ]
}

/// This page's working controls: the combos open their menus.
pub(crate) fn act(app: &mut App, a: &SAct) -> bool {
    match *a {
        SAct::Combo(label, x, y) => app.audio_menu(label, x, y),
        _ => return false,
    }
    true
}

impl App {
    /// Fills the Audio page: the output devices, the audio settings and the status (spec: ui/settings.md "Audio").
    pub(crate) fn audio_page(&mut self) {
        let (devices, default) = crate::audio_cfg::devices();
        let cfg = crate::audio_cfg::AudioCfg::current();
        let st = &mut self.state.settings;
        st.device = match &cfg.device {
            Some(d) if devices.contains(d) => d.clone(),
            Some(d) => format!("{d} (not found: default device)"),
            None => format!("Default ({})", default.as_deref().unwrap_or("no output device")),
        };
        st.devices = devices;
        st.audio = cfg;
        st.rate = crate::audio_cfg::rate();
        self.audio_status();
    }

    /// The Audio page's status lines from the stream that is open; returns whether they changed.
    pub(crate) fn audio_status(&mut self) -> bool {
        let tp = &self.transport;
        let host = cpal::default_host().id().name();
        let first = match tp.opened() {
            Some((name, rate, buffer)) => {
                let block = match tp.block() {
                    0 => String::new(),
                    b => format!(", blocks of {}", buffer_text(b, rate)),
                };
                let buffer = buffer.map_or(String::new(), |b| format!(", buffer {}", buffer_text(b, rate)));
                format!("Status: {name} open at {rate} Hz{block}{buffer} ({host})")
            }
            None => format!("Status: opens with the first note or Play ({host})"),
        };
        let second = match tp.underruns() {
            0 => "No dropouts so far.".to_string(),
            n => format!("Dropouts so far: {n} (device blocks the engine could not fill in time; a longer buffer helps)"),
        };
        let st = &mut self.state.settings;
        let changed = st.status[0] != first || st.status[1] != second;
        st.status = [first, second];
        changed
    }

    /// The menu of an Audio page combo, under its box (`x`, `y`).
    pub(crate) fn audio_menu(&mut self, label: &str, x: f32, y: f32) {
        use crate::audio_cfg::{Set, BUFFERS, RATES};
        use crate::playlist::{Cmd, Mark, PItem, Popup};
        let st = &self.state.settings;
        let cfg = &st.audio;
        let rate = st.rate.unwrap_or(44100);
        let items: Vec<PItem> = match label {
            OUTPUT_DEVICE => std::iter::once(PItem::new("Default device", Cmd::Audio(Set::Device(None))).mark(Mark::Radio(cfg.device.is_none())))
                .chain(st.devices.iter().map(|d| PItem::new(d.clone(), Cmd::Audio(Set::Device(Some(d.clone())))).mark(Mark::Radio(cfg.device.as_ref() == Some(d)))))
                .collect(),
            SAMPLE_RATE => std::iter::once(PItem::new("Device rate", Cmd::Audio(Set::Rate(None))).mark(Mark::Radio(cfg.rate.is_none())))
                .chain(RATES.iter().map(|&r| PItem::new(format!("{r} Hz"), Cmd::Audio(Set::Rate(Some(r)))).mark(Mark::Radio(cfg.rate == Some(r)))))
                .collect(),
            BUFFER_LENGTH => std::iter::once(PItem::new("Device default", Cmd::Audio(Set::Buffer(None))).mark(Mark::Radio(cfg.buffer.is_none())))
                .chain(BUFFERS.iter().map(|&b| {
                    PItem::new(buffer_text(b, rate), Cmd::Audio(Set::Buffer(Some(b)))).mark(Mark::Radio(cfg.buffer == Some(b)))
                }))
                .collect(),
            _ => return,
        };
        self.state.playlist.popup = Some(Popup { x, y: y + 1.0, items, sub: None });
        self.invalidate();
    }

    /// Changes one audio setting: kept for the next start, hosted plugins move to the new rate, and the open streams
    /// move to the new output (spec: ui/settings.md "Audio").
    pub fn set_audio(&mut self, s: crate::audio_cfg::Set) {
        let cfg = crate::audio_cfg::AudioCfg::current().with(s);
        cfg.apply();
        if let Some(rate) = crate::audio_cfg::rate() {
            self.plugins.set_rate(rate);
        }
        self.transport.reopen();
        self.audio_page();
        let st = &self.state.settings;
        self.state.message = Some(format!("Audio: {}, {} Hz", st.device, st.rate.unwrap_or(0)));
        self.invalidate();
    }
}
