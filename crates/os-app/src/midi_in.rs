//! MIDI controller input and the MIDI settings. Input devices open at start (each one can be switched off on
//! Settings > MIDI); note messages play the selected channel like the typing keyboard does, and controllers, pitch
//! bend, program changes and MIDI clock are parsed into `Msg` for whatever follows them. The MIDI settings (which
//! devices are on, their ports, the velocity links, the special channels, the OPTIONS > MIDI switches) are a program
//! setting kept in the user folder (`midi.txt`), like the metronome's.
//!
// spec: ui/midi-input.md (Lemon Studio design), ui/settings.md "MIDI"

use std::sync::mpsc::{channel, Receiver, Sender};

/// A message from a controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    /// Key and velocity (1..=127), any MIDI channel but the special ones below.
    On(u8, u8),
    Off(u8),
    /// Control change: MIDI channel (0..=15), controller number, value.
    Control { channel: u8, cc: u8, value: u8 },
    /// Pitch bend: MIDI channel, -8192..=8191 (0 = centre).
    PitchBend { channel: u8, value: i16 },
    /// Program change: MIDI channel, program number.
    Program { channel: u8, program: u8 },
    /// MIDI clock: 24 per quarter note, and the transport messages that go with it.
    Clock,
    Start,
    Continue,
    Stop,
    /// Song position pointer, in sixteenth notes.
    SongPosition(u16),
    /// A note on the Omni preview channel (spec: ui/settings.md "MIDI"): key, velocity; it previews the channel at that
    /// position in the rack instead of playing the selected one.
    OmniOn(u8, u8),
    OmniOff(u8),
    /// A note on the Song marker jump channel: the key picks the marker to jump to.
    MarkerJump(u8),
    /// A note on the Generator muting channel: the key picks the channel whose mute toggles.
    MuteToggle(u8),
}

/// The message in a MIDI message's bytes, as the device sent it (no channel routing). Note on with velocity 0 is a note
/// off; SysEx, active sensing, MIDI time code and anything incomplete are none.
pub fn parse_bytes(bytes: &[u8]) -> Option<Msg> {
    let (&status, rest) = bytes.split_first()?;
    let d = |i: usize| rest.get(i).map(|b| b & 0x7f);
    let channel = status & 0x0f;
    Some(match status {
        0xf8 => Msg::Clock,
        0xfa => Msg::Start,
        0xfb => Msg::Continue,
        0xfc => Msg::Stop,
        0xf2 => Msg::SongPosition(u16::from(d(0)?) | (u16::from(d(1)?) << 7)),
        0xf0..=0xff => return None,
        _ => match status & 0xf0 {
            0x90 if d(1)? > 0 => Msg::On(d(0)?, d(1)?),
            0x80 | 0x90 => Msg::Off(d(0)?),
            0xb0 => Msg::Control { channel, cc: d(0)?, value: d(1)? },
            0xc0 => Msg::Program { channel, program: d(0)? },
            0xe0 => Msg::PitchBend { channel, value: ((i16::from(d(1)?) << 7) | i16::from(d(0)?)) - 8192 },
            _ => return None,
        },
    })
}

/// The note event in a short MIDI message (status, data 1, data 2), if it is one.
#[cfg(test)]
pub fn parse(status: u8, d1: u8, d2: u8) -> Option<Msg> {
    parse_bytes(&[status, d1, d2]).filter(|m| matches!(m, Msg::On(..) | Msg::Off(_)))
}

/// Velocity of a note on when "Link note on velocity" is off: every note plays at the typing keyboard's velocity
/// (Lemon Studio choice).
pub const FIXED_VELOCITY: u8 = 100;

/// What `bytes` means under the settings `cfg`: notes on a special channel become its message (Omni preview, Song marker
/// jump, Generator muting; spec: ui/settings.md "MIDI"), and a note on plays at `FIXED_VELOCITY` when velocity is not
/// linked.
pub fn route(cfg: &Cfg, bytes: &[u8]) -> Option<Msg> {
    let m = parse_bytes(bytes)?;
    let velocity = |v: u8| if cfg.velocity { v } else { FIXED_VELOCITY };
    // MIDI channel 1..=16 of a note; 0 means "---" in the settings, so it never matches.
    let ch = (bytes[0] & 0x0f) + 1;
    Some(match m {
        Msg::On(k, v) if ch == cfg.omni => Msg::OmniOn(k, velocity(v)),
        Msg::Off(k) if ch == cfg.omni => Msg::OmniOff(k),
        Msg::On(k, _) if ch == cfg.marker => Msg::MarkerJump(k),
        Msg::On(k, _) if ch == cfg.muting => Msg::MuteToggle(k),
        Msg::Off(_) if ch == cfg.marker || ch == cfg.muting => return None,
        Msg::On(k, v) => Msg::On(k, velocity(v)),
        m => m,
    })
}

// ---------------------------------------------------------------------------------------------------------------
// Settings (`midi.txt`).

/// One device's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dev {
    /// Input: its notes play. Output: it is opened.
    pub on: bool,
    /// MIDI port number (0..=255), `None` = "---". Kept for what routes by port (plugins listening on a port, the MIDI
    /// Out channel); nothing does yet.
    pub port: Option<u8>,
    /// Output: "Send master sync" (MIDI clock and start / stop follow the transport).
    pub sync: bool,
}

/// An input device not seen before is on, as every input was before there were settings (Lemon Studio choice).
pub const NEW_INPUT: Dev = Dev { on: true, port: None, sync: false };
/// An output device not seen before is off: nothing is sent to a device until it is switched on.
pub const NEW_OUTPUT: Dev = Dev { on: false, port: None, sync: false };

/// The MIDI settings (Settings > MIDI and OPTIONS > MIDI).
#[derive(Debug, Clone, PartialEq)]
pub struct Cfg {
    /// Devices seen so far, by name (a device that is not connected now keeps its settings).
    pub inputs: Vec<(String, Dev)>,
    pub outputs: Vec<(String, Dev)>,
    /// "Link note on velocity to": on = Velocity, off = none (notes play at `FIXED_VELOCITY`).
    pub velocity: bool,
    /// "Link release velocity to": on = Release, off = none. Kept: Lemon's notes have no release velocity yet.
    pub release: bool,
    /// Omni preview / Song marker jump / Generator muting MIDI channel: 1..=16, 0 = "---".
    pub omni: u8,
    pub marker: u8,
    pub muting: u8,
    /// OPTIONS > Enable MIDI remote control (kept for controller linking), Enable MIDI output, Enable MIDI master sync.
    pub remote: bool,
    pub output: bool,
    pub sync: bool,
}

impl Default for Cfg {
    /// Velocity links on, no special channels, the OPTIONS switches on: outputs still send nothing until one is
    /// switched on on Settings > MIDI (Lemon Studio choice).
    fn default() -> Cfg {
        Cfg { inputs: Vec::new(), outputs: Vec::new(), velocity: true, release: true, omni: 0, marker: 0, muting: 0, remote: true, output: true, sync: true }
    }
}

fn find(list: &[(String, Dev)], name: &str) -> Option<Dev> {
    list.iter().find(|(n, _)| n == name).map(|&(_, d)| d)
}

fn put(list: &mut Vec<(String, Dev)>, name: &str, d: Dev) {
    match list.iter_mut().find(|(n, _)| n == name) {
        Some(e) => e.1 = d,
        None => list.push((name.to_string(), d)),
    }
}

impl Cfg {
    pub fn input(&self, name: &str) -> Dev {
        find(&self.inputs, name).unwrap_or(NEW_INPUT)
    }

    pub fn output(&self, name: &str) -> Dev {
        find(&self.outputs, name).unwrap_or(NEW_OUTPUT)
    }

    pub fn set_input(&mut self, name: &str, d: Dev) {
        put(&mut self.inputs, name, d);
    }

    pub fn set_output(&mut self, name: &str, d: Dev) {
        put(&mut self.outputs, name, d);
    }

    /// `key=value` lines: the switches (0 / 1), the channels (0..=16) and one `in=` / `out=` line per device
    /// (`on,port,sync,name`, port `---` for none). Anything else is ignored.
    pub fn parse(text: &str) -> Cfg {
        let mut c = Cfg::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let flag = |v: &str| v.trim() == "1";
            let chan = |v: &str| v.trim().parse::<u8>().ok().filter(|&n| n <= 16).unwrap_or(0);
            match k.trim() {
                "velocity" => c.velocity = flag(v),
                "release" => c.release = flag(v),
                "omni" => c.omni = chan(v),
                "marker" => c.marker = chan(v),
                "muting" => c.muting = chan(v),
                "remote" => c.remote = flag(v),
                "output" => c.output = flag(v),
                "sync" => c.sync = flag(v),
                k @ ("in" | "out") => {
                    let mut f = v.splitn(4, ',');
                    let (Some(on), Some(port), Some(sync), Some(name)) = (f.next(), f.next(), f.next(), f.next()) else { continue };
                    if name.is_empty() {
                        continue;
                    }
                    let d = Dev { on: flag(on), port: port.trim().parse().ok(), sync: flag(sync) };
                    if k == "in" {
                        c.set_input(name, d);
                    } else {
                        c.set_output(name, d);
                    }
                }
                _ => {}
            }
        }
        c
    }

    pub fn text(&self) -> String {
        let b = |v: bool| u8::from(v);
        let mut t = format!(
            "velocity={}\nrelease={}\nomni={}\nmarker={}\nmuting={}\nremote={}\noutput={}\nsync={}\n",
            b(self.velocity),
            b(self.release),
            self.omni,
            self.marker,
            self.muting,
            b(self.remote),
            b(self.output),
            b(self.sync)
        );
        for (k, list) in [("in", &self.inputs), ("out", &self.outputs)] {
            for (name, d) in list {
                let port = d.port.map_or("---".to_string(), |p| p.to_string());
                t += &format!("{k}={},{port},{},{}\n", b(d.on), b(d.sync), name.replace('\n', " "));
            }
        }
        t
    }
}

/// The settings, the devices present (from the last scan) and the device each list of the settings page has selected.
#[derive(Debug, Default)]
pub struct Midi {
    pub cfg: Cfg,
    pub ins: Vec<String>,
    pub outs: Vec<String>,
    pub sel_in: usize,
    pub sel_out: usize,
}

#[cfg(not(test))]
static MIDI: std::sync::OnceLock<std::sync::Mutex<Midi>> = std::sync::OnceLock::new();

/// Runs `f` on the MIDI settings: read from `midi.txt` the first time (tests: the defaults, one per test thread).
pub fn with<R>(f: impl FnOnce(&mut Midi) -> R) -> R {
    #[cfg(test)]
    {
        thread_local!(static TEST: std::cell::RefCell<Midi> = std::cell::RefCell::default());
        TEST.with(|m| f(&mut m.borrow_mut()))
    }
    #[cfg(not(test))]
    {
        let m = MIDI.get_or_init(|| std::sync::Mutex::new(Midi { cfg: load(), ..Midi::default() }));
        let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut g)
    }
}

/// The settings now.
pub fn cfg() -> Cfg {
    with(|m| m.cfg.clone())
}

/// Changes the settings and keeps them.
pub fn change(f: impl FnOnce(&mut Cfg)) {
    let text = with(|m| {
        f(&mut m.cfg);
        m.cfg.text()
    });
    save(&text);
}

#[cfg_attr(test, allow(dead_code))]
fn load() -> Cfg {
    os_paths::user_file("midi.txt").and_then(|f| std::fs::read_to_string(f).ok()).map(|t| Cfg::parse(&t)).unwrap_or_default()
}

fn save(text: &str) {
    if cfg!(test) {
        return;
    }
    if let Some(dir) = os_paths::user_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("midi.txt"), text);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Input devices.

/// A raw message from a device's driver thread: up to three bytes and how many there are.
type Raw = ([u8; 3], usize);

/// The open input devices and what they sent.
pub struct MidiIn {
    rx: Receiver<Raw>,
    tx: Sender<Raw>,
    /// Called from a driver's thread after a message; `None`: no devices are opened (tests, screenshots).
    wake: Option<fn()>,
    /// Names of the devices that opened.
    pub names: Vec<String>,
    /// Open devices; dropping one closes it.
    _ports: Vec<midir::MidiInputConnection<u32>>,
}

impl MidiIn {
    /// Lists the MIDI devices and opens every input device that is on (a device another program holds is skipped).
    /// `wake` is called from the driver's thread after each message, so the UI handles it at once. Devices come
    /// through `midir`: WinMM on Windows, ALSA on Linux, CoreMIDI on macOS, Web MIDI in a browser.
    pub fn open(wake: fn()) -> MidiIn {
        let (tx, rx) = channel();
        let mut m = MidiIn { rx, tx, wake: Some(wake), names: Vec::new(), _ports: Vec::new() };
        m.refresh();
        m
    }

    /// No devices: for tests and screenshots.
    pub fn none() -> MidiIn {
        let (tx, rx) = channel();
        MidiIn { rx, tx, wake: None, names: Vec::new(), _ports: Vec::new() }
    }

    /// Lists the devices again (Settings > MIDI > Refresh device list) and reopens the inputs that are on.
    pub fn refresh(&mut self) {
        if self.wake.is_none() {
            return;
        }
        let ins = input_names();
        let outs = crate::midi_out::port_names();
        with(|m| {
            m.ins = ins;
            m.outs = outs;
            m.sel_in = m.sel_in.min(m.ins.len().saturating_sub(1));
            m.sel_out = m.sel_out.min(m.outs.len().saturating_sub(1));
        });
        self.reopen();
    }

    /// Closes the inputs and opens the ones that are on now (after a device is switched on or off).
    pub fn reopen(&mut self) {
        let Some(wake) = self.wake else { return };
        self._ports.clear();
        let cfg = cfg();
        let (ports, names) = open_some(&self.tx, wake, |name| cfg.input(name).on);
        self._ports = ports;
        self.names = names;
    }

    /// Messages received since the last call, oldest first, read with the settings now (`route`).
    pub fn poll(&self) -> Vec<Msg> {
        let raw: Vec<Raw> = self.rx.try_iter().collect();
        if raw.is_empty() {
            return Vec::new();
        }
        let cfg = cfg();
        raw.iter().filter_map(|(b, n)| route(&cfg, &b[..*n])).collect()
    }
}

/// The name Lemon Studio's connections show in the system's MIDI routing (ALSA / JACK clients).
pub(crate) const CLIENT: &str = "Lemon Studio";

/// Whether a port is one of Lemon Studio's own connections (ALSA lists them among the other side's ports).
pub(crate) fn own_port(name: &str) -> bool {
    name.starts_with(CLIENT)
}

/// The input devices present.
fn input_names() -> Vec<String> {
    let Ok(list) = midir::MidiInput::new(CLIENT) else { return Vec::new() };
    list.ports().iter().filter_map(|p| list.port_name(p).ok()).filter(|n| !own_port(n)).collect()
}

/// Connects to the input ports `want` picks. A connection takes its `MidiInput` along, so each port gets its own.
fn open_some(tx: &Sender<Raw>, wake: fn(), want: impl Fn(&str) -> bool) -> (Vec<midir::MidiInputConnection<u32>>, Vec<String>) {
    let (mut conns, mut names) = (Vec::new(), Vec::new());
    let Ok(list) = midir::MidiInput::new(CLIENT) else { return (conns, names) };
    for port in list.ports() {
        let Ok(name) = list.port_name(&port) else { continue };
        if own_port(&name) || !want(&name) {
            continue;
        }
        let Ok(mut input) = midir::MidiInput::new(CLIENT) else { break };
        // SysEx and active sensing are dropped by the driver; clock and song position come through.
        input.ignore(midir::Ignore::SysexAndActiveSense);
        let Some(port) = input.find_port_by_id(&port.id()) else { continue };
        let tx = tx.clone();
        // Runs on the driver's thread: only queues the bytes and wakes the UI. Clock comes 24 times a beat: it wakes
        // the UI once a beat (the counter), the UI reads the rest with whatever comes next.
        let note = move |_stamp: u64, bytes: &[u8], clocks: &mut u32| {
            if bytes.is_empty() || bytes.len() > 3 || bytes[0] == 0xf0 {
                return;
            }
            let mut b = [0; 3];
            b[..bytes.len()].copy_from_slice(bytes);
            if tx.send((b, bytes.len())).is_err() {
                return;
            }
            if bytes[0] == 0xf8 {
                *clocks = (*clocks + 1) % 24;
                if *clocks != 0 {
                    return;
                }
            }
            wake();
        };
        if let Ok(conn) = input.connect(&port, CLIENT, note, 0) {
            conns.push(conn);
            names.push(name);
        }
    }
    (conns, names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_messages() {
        assert_eq!(parse(0x90, 60, 100), Some(Msg::On(60, 100)));
        assert_eq!(parse(0x93, 61, 1), Some(Msg::On(61, 1)), "any MIDI channel");
        assert_eq!(parse(0x90, 60, 0), Some(Msg::Off(60)), "velocity 0 = note off");
        assert_eq!(parse(0x85, 60, 64), Some(Msg::Off(60)));
        assert_eq!(parse(0xb0, 7, 100), None, "not a note");
    }

    #[test]
    fn controller_bend_program_and_clock_messages() {
        assert_eq!(parse_bytes(&[0xb2, 7, 100]), Some(Msg::Control { channel: 2, cc: 7, value: 100 }));
        assert_eq!(parse_bytes(&[0xc5, 12]), Some(Msg::Program { channel: 5, program: 12 }), "two bytes");
        assert_eq!(parse_bytes(&[0xe0, 0, 0x40]), Some(Msg::PitchBend { channel: 0, value: 0 }), "centre");
        assert_eq!(parse_bytes(&[0xe1, 0, 0]), Some(Msg::PitchBend { channel: 1, value: -8192 }));
        assert_eq!(parse_bytes(&[0xef, 0x7f, 0x7f]), Some(Msg::PitchBend { channel: 15, value: 8191 }));
        assert_eq!(parse_bytes(&[0xf8]), Some(Msg::Clock));
        assert_eq!(parse_bytes(&[0xfa]), Some(Msg::Start));
        assert_eq!(parse_bytes(&[0xfb]), Some(Msg::Continue));
        assert_eq!(parse_bytes(&[0xfc]), Some(Msg::Stop));
        assert_eq!(parse_bytes(&[0xf2, 0x10, 0x01]), Some(Msg::SongPosition(0x90)));
        assert_eq!(parse_bytes(&[0xfe]), None, "active sensing");
        assert_eq!(parse_bytes(&[0xf0, 0x7e, 0xf7]), None, "SysEx");
        assert_eq!(parse_bytes(&[0xa0, 60, 10]), None, "aftertouch is not read");
        assert_eq!(parse_bytes(&[0xb0, 7]), None, "incomplete");
        assert_eq!(parse_bytes(&[]), None);
    }

    #[test]
    fn special_channels_and_velocity_link() {
        let mut c = Cfg { omni: 2, marker: 3, muting: 4, ..Cfg::default() };
        assert_eq!(route(&c, &[0x90, 60, 90]), Some(Msg::On(60, 90)), "channel 1 plays");
        assert_eq!(route(&c, &[0x91, 60, 90]), Some(Msg::OmniOn(60, 90)));
        assert_eq!(route(&c, &[0x81, 60, 0]), Some(Msg::OmniOff(60)));
        assert_eq!(route(&c, &[0x92, 5, 90]), Some(Msg::MarkerJump(5)));
        assert_eq!(route(&c, &[0x82, 5, 0]), None, "a marker key's release does nothing");
        assert_eq!(route(&c, &[0x93, 7, 90]), Some(Msg::MuteToggle(7)));
        assert_eq!(route(&c, &[0x93, 7, 0]), None);
        assert_eq!(route(&c, &[0xb1, 1, 2]), Some(Msg::Control { channel: 1, cc: 1, value: 2 }), "only notes are routed");
        c.velocity = false;
        assert_eq!(route(&c, &[0x90, 60, 30]), Some(Msg::On(60, FIXED_VELOCITY)));
        assert_eq!(route(&c, &[0x91, 60, 30]), Some(Msg::OmniOn(60, FIXED_VELOCITY)));
        let d = Cfg::default();
        assert_eq!(route(&d, &[0x9f, 60, 30]), Some(Msg::On(60, 30)), "no special channel by default");
    }

    #[test]
    fn settings_file_round_trips() {
        let d = Cfg::parse("");
        assert_eq!(d, Cfg::default());
        assert!(d.input("Keys").on && !d.output("Synth").on, "new inputs are on, new outputs off");
        let mut c = Cfg { velocity: false, omni: 16, marker: 1, remote: false, sync: false, ..Cfg::default() };
        c.set_input("Keys, port=1", Dev { on: false, port: Some(3), sync: false });
        c.set_output("Synth", Dev { on: true, port: None, sync: true });
        c.set_output("Synth", Dev { on: true, port: Some(255), sync: true });
        let t = c.text();
        assert!(t.contains("in=0,3,0,Keys, port=1\n") && t.contains("out=1,255,1,Synth\n"), "{t}");
        assert_eq!(Cfg::parse(&t), c);
        assert_eq!(c.outputs.len(), 1, "a device is kept once");
        let odd = Cfg::parse("omni=17\nmuting=x\nin=1,---\nout=1,999,0,Big\njunk\n");
        assert_eq!((odd.omni, odd.muting), (0, 0), "channels are 1..=16");
        assert!(odd.inputs.is_empty(), "short device line");
        assert_eq!(odd.output("Big").port, None, "ports are 0..=255");
    }

    #[test]
    fn settings_change_on_this_thread_only() {
        change(|c| c.set_input("Keys", Dev { on: false, port: Some(1), sync: false }));
        assert!(!cfg().input("Keys").on);
        let other = std::thread::spawn(|| cfg().input("Keys").on).join().unwrap();
        assert!(other, "tests side by side keep their own settings");
    }

    /// Notes sent to a loopback MIDI port (an output that is also an input: a loopMIDI port on Windows, ALSA's
    /// "Midi Through" on Linux) arrive and wake the UI. Needs such a port: `cargo test -p os-app midi_loopback --
    /// --ignored`.
    #[test]
    #[ignore]
    fn midi_loopback() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static WAKES: AtomicUsize = AtomicUsize::new(0);
        fn wake() {
            WAKES.fetch_add(1, Ordering::Relaxed);
        }
        let midi = MidiIn::open(wake);
        let out = midir::MidiOutput::new("Lemon Studio test").expect("MIDI output");
        let port = out.ports().into_iter().find(|p| out.port_name(p).is_ok_and(|n| midi.names.contains(&n)));
        let Some(port) = port else { panic!("no loopback port among the inputs {:?}", midi.names) };
        let mut conn = out.connect(&port, "loopback").expect("open the loopback output");
        conn.send(&[0x90, 60, 100]).unwrap();
        conn.send(&[0x80, 60, 0]).unwrap();
        conn.send(&[0xb0, 1, 64]).unwrap();
        conn.send(&[0xf8]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        conn.close();
        assert_eq!(midi.poll(), vec![Msg::On(60, 100), Msg::Off(60), Msg::Control { channel: 0, cc: 1, value: 64 }, Msg::Clock]);
        assert!(WAKES.load(Ordering::Relaxed) >= 3, "each note and controller wakes the UI");
    }
}
