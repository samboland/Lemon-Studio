//! MIDI output: the output devices that are on (Settings > MIDI) are opened, and the ones that "Send master sync" get
//! MIDI clock (24 per quarter note) with start / continue / stop and the song position, following the transport. A
//! thread of its own sends the clock, so it does not wait for the UI's frames; the UI tells it the transport state
//! each frame (`follow`).
//!
// spec: ui/settings.md "MIDI", ui/midi-input.md (Lemon Studio design)

use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::midi_in::{own_port, CLIENT};

/// The output devices present.
pub fn port_names() -> Vec<String> {
    let Ok(list) = midir::MidiOutput::new(CLIENT) else { return Vec::new() };
    list.ports().iter().filter_map(|p| list.port_name(p).ok()).filter(|n| !own_port(n)).collect()
}

/// The transport as the clock follows it: running (playing; paused and stopped are not), the position in ticks, the
/// tempo and the project's ticks per quarter note.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Snap {
    pub running: bool,
    pub tick: f64,
    pub tempo: f64,
    pub ppq: f64,
}

/// MIDI clock pulses per quarter note, and per sixteenth (the song position's unit).
const PPQN: f64 = 24.0;
const PER_16TH: i64 = 6;

pub const CLOCK: u8 = 0xf8;
pub const START: u8 = 0xfa;
pub const CONTINUE: u8 = 0xfb;
pub const STOP: u8 = 0xfc;

/// Song position pointer message: `sixteenths` from the start.
fn song_position(sixteenths: i64) -> Vec<u8> {
    let v = sixteenths.clamp(0, 0x3fff) as u16;
    vec![0xf2, (v & 0x7f) as u8, (v >> 7) as u8]
}

/// The clock: where the running transport was at `at` (in pulses), how fast it goes, the next pulse to send.
#[derive(Debug, Default)]
pub struct Clock {
    run: Option<(f64, Instant, f64)>,
    next: i64,
}

impl Clock {
    /// The transport's position in pulses at `now`, while running.
    fn pulse(&self, now: Instant) -> Option<f64> {
        let (p, at, rate) = self.run?;
        Some(p + now.saturating_duration_since(at).as_secs_f64() * rate)
    }

    /// Song position and continue (start at the very beginning): pulses resume from the next sixteenth.
    fn locate(&mut self, p: f64) -> Vec<Vec<u8>> {
        let sixteenth = (p / PER_16TH as f64 - 1e-6).ceil().max(0.0) as i64;
        self.next = sixteenth * PER_16TH;
        vec![song_position(sixteenth), vec![if sixteenth == 0 { START } else { CONTINUE }]]
    }

    /// Follows the transport: the messages to send now (start / continue with the song position when it starts or
    /// jumps, stop when it stops). A jump is the position moving back more than half a beat (a loop, a click on the
    /// timeline) or ahead more than two beats from where the clock is.
    pub fn update(&mut self, s: Snap, now: Instant) -> Vec<Vec<u8>> {
        let p = s.tick * PPQN / s.ppq.max(1.0);
        let rate = s.tempo.max(1.0) * PPQN / 60.0;
        let was = self.run.is_some();
        if !s.running {
            self.run = None;
            return if was { vec![vec![STOP]] } else { Vec::new() };
        }
        self.run = Some((p, now, rate));
        if !was {
            return self.locate(p);
        }
        let ahead = p - self.next as f64;
        if ahead < -PPQN / 2.0 || ahead > 2.0 * PPQN {
            let mut v = vec![vec![STOP]];
            v.extend(self.locate(p));
            return v;
        }
        Vec::new()
    }

    /// How many clock pulses are due at `now` (at most a beat's worth: after a stall the clock skips ahead).
    pub fn due(&mut self, now: Instant) -> usize {
        let Some(p) = self.pulse(now) else { return 0 };
        let n = p.floor() as i64 - self.next + 1;
        if n <= 0 {
            return 0;
        }
        self.next += n;
        n.min(PPQN as i64) as usize
    }

    /// When the next pulse is due, while running.
    pub fn next_at(&self) -> Option<Instant> {
        let (p, at, rate) = self.run?;
        let secs = ((self.next as f64 - p) / rate).max(0.0);
        Some(at + Duration::from_secs_f64(secs))
    }
}

/// What the sending thread is told.
enum Cmd {
    /// Open these devices (name, send master sync) and close the others.
    Open(Vec<(String, bool)>),
    Snap(Snap),
}

/// The sending thread and what it was last told.
struct Out {
    tx: Sender<Cmd>,
    open: Vec<(String, bool)>,
    last: Snap,
}

static OUT: Mutex<Option<Out>> = Mutex::new(None);

/// The devices to open now: the outputs that are on and present (none when OPTIONS > Enable MIDI output is off), with
/// whether each sends master sync (OPTIONS > Enable MIDI master sync and the device's "Send master sync").
pub fn wanted(m: &crate::midi_in::Midi) -> Vec<(String, bool)> {
    let c = &m.cfg;
    if !c.output {
        return Vec::new();
    }
    m.outs.iter().map(|n| (n, c.output(n))).filter(|(_, d)| d.on).map(|(n, d)| (n.clone(), c.sync && d.sync)).collect()
}

/// Called every UI frame: opens and closes output devices as the settings say and tells the clock where the transport
/// is. Nothing runs until a device is on (tests: never; they have no devices).
pub fn follow(tp: &crate::transport::Transport, s: &crate::app::State) {
    if cfg!(test) {
        return;
    }
    let want = crate::midi_in::with(|m| wanted(m));
    let Ok(mut out) = OUT.lock() else { return };
    if want.is_empty() && out.is_none() {
        return;
    }
    let snap = Snap { running: tp.state() == crate::transport::State::Playing, tick: tp.tick(), tempo: s.tempo, ppq: f64::from(s.project.ppq.max(1)) };
    let o = out.get_or_insert_with(|| Out { tx: spawn(), open: Vec::new(), last: Snap::default() });
    if o.open != want {
        o.open = want.clone();
        let _ = o.tx.send(Cmd::Open(want));
    }
    if snap != o.last {
        o.last = snap;
        let _ = o.tx.send(Cmd::Snap(snap));
    }
}

/// The sending thread: owns the open devices and the clock.
fn spawn() -> Sender<Cmd> {
    let (tx, rx) = channel::<Cmd>();
    let _ = std::thread::Builder::new().name("midi-out".into()).spawn(move || {
        let mut conns: Vec<(String, bool, midir::MidiOutputConnection)> = Vec::new();
        let mut clock = Clock::default();
        loop {
            let now = Instant::now();
            let wait = clock.next_at().map_or(Duration::from_millis(250), |t| t.saturating_duration_since(now));
            let mut send = Vec::new();
            match rx.recv_timeout(wait) {
                Ok(Cmd::Open(list)) => {
                    // A device that stops sending sync hears stop first.
                    for (name, sync, c) in &mut conns {
                        if *sync && !list.iter().any(|(n, s)| n == name && *s) {
                            let _ = c.send(&[STOP]);
                        }
                    }
                    conns.retain(|(n, _, _)| list.iter().any(|(m, _)| m == n));
                    for (name, sync) in list {
                        match conns.iter_mut().find(|(n, _, _)| *n == name) {
                            Some(c) => c.1 = sync,
                            None => {
                                if let Some(c) = connect(&name) {
                                    conns.push((name, sync, c));
                                }
                            }
                        }
                    }
                }
                Ok(Cmd::Snap(s)) => send = clock.update(s, Instant::now()),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            send.extend(std::iter::repeat_n(vec![CLOCK], clock.due(Instant::now())));
            for (_, sync, c) in &mut conns {
                if *sync {
                    for m in &send {
                        let _ = c.send(m);
                    }
                }
            }
        }
    });
    tx
}

/// Opens output device `name`.
fn connect(name: &str) -> Option<midir::MidiOutputConnection> {
    let out = midir::MidiOutput::new(CLIENT).ok()?;
    let port = out.ports().into_iter().find(|p| out.port_name(p).is_ok_and(|n| n == name))?;
    out.connect(&port, CLIENT).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi_in::{Cfg, Dev, Midi};

    fn snap(running: bool, tick: f64) -> Snap {
        Snap { running, tick, tempo: 120.0, ppq: 96.0 }
    }

    #[test]
    fn clock_starts_runs_and_stops_with_the_transport() {
        let t0 = Instant::now();
        let mut c = Clock::default();
        assert!(c.update(snap(false, 0.0), t0).is_empty(), "stopped: nothing");
        assert_eq!(c.due(t0), 0);
        assert_eq!(c.update(snap(true, 0.0), t0), vec![vec![0xf2, 0, 0], vec![START]]);
        assert_eq!(c.due(t0), 1, "the first pulse is the downbeat");
        // 120 BPM: 48 pulses a second.
        let half = t0 + Duration::from_millis(500);
        assert_eq!(c.due(half), 24);
        assert_eq!(c.due(half), 0, "nothing twice");
        assert!(c.next_at().is_some_and(|t| t > half && t <= half + Duration::from_millis(21)));
        // The transport reports a little behind the clock: no jump.
        assert!(c.update(snap(true, 94.0), half).is_empty());
        assert_eq!(c.update(snap(false, 48.0), half), vec![vec![STOP]]);
        assert_eq!(c.due(half + Duration::from_secs(1)), 0, "stopped");
        assert!(c.next_at().is_none());
    }

    #[test]
    fn clock_continues_from_the_song_position_after_a_jump() {
        let t0 = Instant::now();
        let mut c = Clock::default();
        // Bar 2 (ppq 96: 384 ticks = 16 sixteenths).
        assert_eq!(c.update(snap(true, 384.0), t0), vec![vec![0xf2, 16, 0], vec![CONTINUE]]);
        assert_eq!(c.due(t0), 1);
        // Mid-sixteenth: the clock resumes from the next one.
        let mut d = Clock::default();
        assert_eq!(d.update(snap(true, 30.0), t0), vec![vec![0xf2, 2, 0], vec![CONTINUE]]);
        assert_eq!(d.due(t0), 0, "waits for the sixteenth");
        // A pattern loops back to its start.
        assert_eq!(c.update(snap(true, 0.0), t0), vec![vec![STOP], vec![0xf2, 0, 0], vec![START]]);
        // Pausing stops, playing on continues.
        assert_eq!(c.update(snap(false, 96.0), t0), vec![vec![STOP]]);
        assert_eq!(c.update(snap(true, 96.0), t0), vec![vec![0xf2, 4, 0], vec![CONTINUE]]);
    }

    #[test]
    fn a_stall_skips_ahead() {
        let t0 = Instant::now();
        let mut c = Clock::default();
        c.update(snap(true, 0.0), t0);
        assert_eq!(c.due(t0 + Duration::from_secs(10)), 24, "at most a beat at once");
        assert_eq!(c.due(t0 + Duration::from_secs(10)), 0, "and the clock is caught up");
    }

    #[test]
    fn outputs_that_are_on_open_and_the_switches_rule() {
        let mut m = Midi { outs: vec!["Synth".into(), "Drums".into(), "Gone?".into()], ..Midi::default() };
        assert!(wanted(&m).is_empty(), "outputs are off at first");
        m.cfg.set_output("Synth", Dev { on: true, port: Some(1), sync: true });
        m.cfg.set_output("Drums", Dev { on: true, port: None, sync: false });
        m.cfg.set_output("Unplugged", Dev { on: true, port: None, sync: true });
        assert_eq!(wanted(&m), vec![("Synth".to_string(), true), ("Drums".to_string(), false)], "present devices only");
        m.cfg.sync = false;
        assert_eq!(wanted(&m), vec![("Synth".to_string(), false), ("Drums".to_string(), false)], "OPTIONS > Enable MIDI master sync");
        m.cfg = Cfg { output: false, ..m.cfg.clone() };
        assert!(wanted(&m).is_empty(), "OPTIONS > Enable MIDI output");
    }

    /// OPTIONS > Enable MIDI remote control / output / master sync work: each toggles its switch in the settings.
    #[test]
    fn options_menu_switches() {
        use crate::views::menus::OPTIONS_MENU;
        let mut app = crate::app::App::new(None);
        for label in ["Enable MIDI remote control", "Enable MIDI output", "Enable MIDI master sync"] {
            assert!(crate::menu_registry::handles(OPTIONS_MENU, label), "{label} works");
            assert_eq!(crate::menu_options::midi_tick(label), Some(true), "{label} is on at first");
            assert!(crate::menu_registry::run(&mut app, OPTIONS_MENU, label));
            assert_eq!(crate::menu_options::midi_tick(label), Some(false));
            assert_eq!(app.state.message, Some(format!("{label}: off")));
        }
        let c = crate::midi_in::cfg();
        assert!(!c.remote && !c.output && !c.sync);
        assert_eq!(crate::menu_options::midi_tick("Metronome"), None);
    }

    /// Clock sent to a loopback port (ALSA's "Midi Through", a loopMIDI port) arrives as start, clock and stop. Needs
    /// such a port: `cargo test -p os-app midi_clock_loopback -- --ignored`.
    #[test]
    #[ignore]
    fn midi_clock_loopback() {
        use crate::midi_in::Msg;
        fn wake() {}
        let input = crate::midi_in::MidiIn::open(wake);
        let name = crate::midi_in::with(|m| m.outs.iter().find(|o| input.names.contains(o)).cloned());
        let Some(name) = name else { panic!("no loopback port among the inputs {:?}", input.names) };
        let mut c = connect(&name).expect("open the loopback output");
        let t0 = Instant::now();
        let mut clock = Clock::default();
        for m in clock.update(snap(true, 0.0), t0) {
            c.send(&m).unwrap();
        }
        for _ in 0..clock.due(t0 + Duration::from_millis(100)) {
            c.send(&[CLOCK]).unwrap();
        }
        for m in clock.update(snap(false, 10.0), t0) {
            c.send(&m).unwrap();
        }
        std::thread::sleep(Duration::from_millis(200));
        let got = input.poll();
        assert_eq!(got.first(), Some(&Msg::SongPosition(0)));
        assert_eq!(got.iter().filter(|m| **m == Msg::Clock).count(), 5);
        assert_eq!(got.last(), Some(&Msg::Stop));
    }
}
