//! Settings > MIDI: output and input devices, external sync. The device lists show every device present with whether
//! it is on and its port; the device the buttons and checks under a list act on is marked ▸ (`midi_in::Midi::sel_in`
//! / `sel_out`). Everything here is kept in `midi.txt` (`midi_in::Cfg`).
// spec: ui/settings.md "MIDI", ui/midi-input.md

use super::{combo, g, pair, txt, Group, Row, SAct};
use crate::app::{App, State};
use crate::midi_in::{self, Cfg, Dev, Midi};

pub const ENABLE_OUTPUT: &str = "Enable output";
pub const SEND_SYNC: &str = "Send master sync";
pub const NEXT_OUTPUT: &str = "Next output device";
pub const OUTPUT_PORT_DOWN: &str = "Output port -";
pub const OUTPUT_PORT_UP: &str = "Output port +";
pub const ENABLE_INPUT: &str = "Enable input";
pub const NEXT_INPUT: &str = "Next input device";
pub const INPUT_PORT_DOWN: &str = "Input port -";
pub const INPUT_PORT_UP: &str = "Input port +";
pub const LINK_VELOCITY: &str = "Link note on velocity";
pub const LINK_RELEASE: &str = "Link release velocity";
pub const REFRESH: &str = "Refresh device list";

/// The checks and buttons of this page that work (`SAct::Check` / `SAct::Button` labels, handled in `act`).
pub const LIVE: &[&str] = &[
    ENABLE_OUTPUT, SEND_SYNC, NEXT_OUTPUT, OUTPUT_PORT_DOWN, OUTPUT_PORT_UP, ENABLE_INPUT, NEXT_INPUT, INPUT_PORT_DOWN,
    INPUT_PORT_UP, LINK_VELOCITY, LINK_RELEASE, REFRESH,
];

fn port_text(p: Option<u8>) -> String {
    p.map_or("---".into(), |p| p.to_string())
}

fn chan_text(c: u8) -> String {
    if c == 0 { "---".into() } else { c.to_string() }
}

/// A device list's rows: name (▸ on the selected one), on / off, port, and for outputs whether it sends sync.
fn rows(names: &[String], sel: usize, dev: impl Fn(&str) -> (String, Option<u8>, Option<bool>), none: &str) -> Vec<Vec<String>> {
    if names.is_empty() {
        return vec![vec![none.to_string()]];
    }
    names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let (state, port, sync) = dev(n);
            let mut r = vec![format!("{} {n}", if i == sel { "▸" } else { " " }), state, port_text(port)];
            if let Some(s) = sync {
                r.push(if s { "on".into() } else { String::new() });
            }
            r
        })
        .collect()
}

/// The MIDI page's groups (spec §4). The device lists' selection, enable and port are buttons and checks under each
/// list, and the velocity links are checks (Lemon Studio choice: the page's lists and value boxes take no clicks).
pub fn page(s: &State) -> Vec<Group> {
    use Row::*;
    let (c, ins, outs, sel_in, sel_out) = midi_in::with(|m| (m.cfg.clone(), m.ins.clone(), m.outs.clone(), m.sel_in, m.sel_out));
    let out = outs.get(sel_out).map_or(midi_in::NEW_OUTPUT, |n| c.output(n));
    let inp = ins.get(sel_in).map_or(midi_in::NEW_INPUT, |n| c.input(n));
    let out_rows = rows(&outs, sel_out, |n| { let d = c.output(n); (if d.on { "on" } else { "off" }.into(), d.port, Some(d.sync)) }, "(no MIDI output devices)");
    // An input that is on but did not open is held by another program.
    let in_rows = rows(&ins, sel_in, |n| {
        let d = c.input(n);
        let state = if !d.on { "off" } else if s.midi_inputs.iter().any(|o| o == n) { "on" } else { "not open" };
        (state.into(), d.port, None)
    }, "(no MIDI input devices)");
    vec![
        g("Output", vec![
            List(vec![("Output devices", 0.0), ("Enabled", 380.0), ("Port", 460.0), ("Sync", 520.0)], out_rows, 6),
            Buttons(vec![(NEXT_OUTPUT, 150.0), (OUTPUT_PORT_DOWN, 110.0), (OUTPUT_PORT_UP, 110.0)]),
            Check2(ENABLE_OUTPUT, out.on, SEND_SYNC, out.sync),
            Check("Send all notes off", true),
            pair(Value("0".into(), "Master sync offset (milliseconds)"), combo("MIDI clock", 168.0, "Synchronization type")),
        ]),
        g("Input", vec![
            List(vec![("Input devices (notes play the selected channel)", 0.0), ("Enabled", 380.0), ("Port", 460.0)], in_rows, 6),
            Buttons(vec![(NEXT_INPUT, 150.0), (INPUT_PORT_DOWN, 110.0), (INPUT_PORT_UP, 110.0)]),
            Check(ENABLE_INPUT, inp.on),
            combo("(generic controller)", 250.0, "Controller type"),
            pair(Check(LINK_VELOCITY, c.velocity), ValueR("Omni preview MIDI channel", chan_text(c.omni))),
            pair(Check(LINK_RELEASE, c.release), ValueR("Song marker jump MIDI channel", chan_text(c.marker))),
            pair(Check("Pickup (takeover mode)", false), ValueR("Performance mode MIDI channel", "---".into())),
            pair(Check("Auto accept detected controller", true), ValueR("Generator muting MIDI channel", chan_text(c.muting))),
            pair(Check("Foot pedal controls note off", true), Check("Toggle on release", false)),
        ]),
        g("External sync", vec![
            pair(combo("Off", 168.0, "External clock sync"), ValueR("External sync offset (milliseconds)", "0".into())),
            txt("Status: off"),
        ]),
        g("", vec![Buttons(vec![(REFRESH, 136.0), ("Update MIDI scripts", 136.0)])]),
    ]
}

/// One port step: down from 0 is "---", up from "---" is 0, 255 at most.
pub fn step_port(p: Option<u8>, up: bool) -> Option<u8> {
    match (p, up) {
        (None, true) => Some(0),
        (None, false) | (Some(0), false) => None,
        (Some(p), true) => Some(p.saturating_add(1)),
        (Some(p), false) => Some(p - 1),
    }
}

/// Changes the selected input (`out` false) or output device's settings and keeps them; false when there is none.
fn change_dev(out: bool, f: impl FnOnce(&mut Dev)) -> bool {
    let name = midi_in::with(|m: &mut Midi| if out { m.outs.get(m.sel_out).cloned() } else { m.ins.get(m.sel_in).cloned() });
    let Some(name) = name else { return false };
    midi_in::change(|c: &mut Cfg| {
        let mut d = if out { c.output(&name) } else { c.input(&name) };
        f(&mut d);
        if out { c.set_output(&name, d) } else { c.set_input(&name, d) }
    });
    true
}

/// This page's working controls (`LIVE`).
pub(crate) fn act(app: &mut App, a: &SAct) -> bool {
    if app.state.settings.tab != super::MIDI {
        return false;
    }
    let (k, button) = match *a {
        SAct::Check(k) => (k, false),
        SAct::Button(k) => (k, true),
        _ => return false,
    };
    let ok = match (k, button) {
        (REFRESH, true) => {
            app.midi.refresh();
            app.state.midi_inputs = app.midi.names.clone();
            let (i, o) = midi_in::with(|m| (m.ins.len(), m.outs.len()));
            app.state.message = Some(format!("MIDI devices: {i} input, {o} output"));
            true
        }
        (NEXT_OUTPUT, true) => midi_in::with(|m| {
            m.sel_out = (m.sel_out + 1) % m.outs.len().max(1);
            !m.outs.is_empty()
        }),
        (NEXT_INPUT, true) => midi_in::with(|m| {
            m.sel_in = (m.sel_in + 1) % m.ins.len().max(1);
            !m.ins.is_empty()
        }),
        (OUTPUT_PORT_DOWN | OUTPUT_PORT_UP, true) => change_dev(true, |d| d.port = step_port(d.port, k == OUTPUT_PORT_UP)),
        (INPUT_PORT_DOWN | INPUT_PORT_UP, true) => change_dev(false, |d| d.port = step_port(d.port, k == INPUT_PORT_UP)),
        (ENABLE_OUTPUT, false) => change_dev(true, |d| d.on = !d.on),
        (SEND_SYNC, false) => change_dev(true, |d| d.sync = !d.sync),
        (ENABLE_INPUT, false) => {
            let ok = change_dev(false, |d| d.on = !d.on);
            app.midi.reopen();
            app.state.midi_inputs = app.midi.names.clone();
            ok
        }
        (LINK_VELOCITY, false) => {
            midi_in::change(|c| c.velocity = !c.velocity);
            true
        }
        (LINK_RELEASE, false) => {
            midi_in::change(|c| c.release = !c.release);
            true
        }
        _ => return false,
    };
    if !ok {
        let what = if matches!(k, ENABLE_OUTPUT | SEND_SYNC | NEXT_OUTPUT | OUTPUT_PORT_DOWN | OUTPUT_PORT_UP) { "output" } else { "input" };
        app.state.message = Some(format!("No MIDI {what} device: connect one and click {REFRESH}"));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Action;
    use crate::views::settings::MIDI;

    #[test]
    fn port_steps() {
        assert_eq!(step_port(None, true), Some(0));
        assert_eq!(step_port(None, false), None);
        assert_eq!(step_port(Some(0), false), None);
        assert_eq!(step_port(Some(4), false), Some(3));
        assert_eq!(step_port(Some(255), true), Some(255));
    }

    /// The page with two devices of each kind (this test thread's settings).
    fn app() -> App {
        midi_in::with(|m| {
            m.ins = vec!["Keys".into(), "Pads".into()];
            m.outs = vec!["Synth".into(), "Drums".into()];
        });
        let mut app = App::new(None);
        app.open_settings(MIDI);
        app
    }

    fn click(app: &mut App, a: SAct) {
        app.act(Action::Settings(a));
    }

    /// The controls of the page work: devices are switched on and off and get ports, the velocity links toggle, and
    /// what changed is in the settings.
    #[test]
    fn devices_get_switched_and_ported() {
        let mut app = app();
        click(&mut app, SAct::Check(ENABLE_INPUT));
        assert!(!midi_in::cfg().input("Keys").on, "the selected input (the first) goes off");
        click(&mut app, SAct::Button(NEXT_INPUT));
        click(&mut app, SAct::Button(INPUT_PORT_UP));
        click(&mut app, SAct::Button(INPUT_PORT_UP));
        let c = midi_in::cfg();
        assert_eq!((c.input("Pads").port, c.input("Keys").port), (Some(1), None));
        click(&mut app, SAct::Button(NEXT_OUTPUT));
        click(&mut app, SAct::Check(ENABLE_OUTPUT));
        click(&mut app, SAct::Check(SEND_SYNC));
        click(&mut app, SAct::Button(OUTPUT_PORT_UP));
        let d = midi_in::cfg().output("Drums");
        assert!(d.on && d.sync && d.port == Some(0));
        assert!(!midi_in::cfg().output("Synth").on);
        let wanted = midi_in::with(|m| crate::midi_out::wanted(m));
        assert_eq!(wanted, vec![("Drums".to_string(), true)], "the output opens and sends sync");
        click(&mut app, SAct::Check(LINK_VELOCITY));
        click(&mut app, SAct::Check(LINK_RELEASE));
        let c = midi_in::cfg();
        assert!(!c.velocity && !c.release);
        // Wraps round to the first device.
        click(&mut app, SAct::Button(NEXT_INPUT));
        assert_eq!(midi_in::with(|m| m.sel_in), 0);
        // Another tab's controls are not this page's.
        app.state.settings.tab = crate::views::settings::AUDIO;
        assert!(!act(&mut app, &SAct::Check(LINK_VELOCITY)));
    }

    #[test]
    fn no_device_says_so() {
        let mut app = App::new(None);
        app.open_settings(MIDI);
        click(&mut app, SAct::Check(ENABLE_OUTPUT));
        assert!(app.state.message.as_deref().is_some_and(|m| m.starts_with("No MIDI output device")), "{:?}", app.state.message);
        click(&mut app, SAct::Button(REFRESH));
        assert_eq!(app.state.message.as_deref(), Some("MIDI devices: 0 input, 0 output"), "no devices are opened in tests");
    }

    /// The page's controls draw live (not greyed placeholders), and the lists show the devices with their settings.
    #[test]
    fn controls_are_live() {
        let mut app = app();
        let Ok(mut c) = os_ui::Canvas::new(1920, 1080, 1.0) else { return };
        app.paint(&mut c, None);
        for k in LIVE {
            assert!(!app.hits.iter().any(|h| h.action == Action::Placeholder(k)), "{k} is greyed");
            let a = if matches!(*k, ENABLE_OUTPUT | SEND_SYNC | ENABLE_INPUT | LINK_VELOCITY | LINK_RELEASE) { SAct::Check(k) } else { SAct::Button(k) };
            assert!(app.hits.iter().any(|h| h.action == Action::Settings(a.clone())), "{k} is not drawn");
        }
        let groups = page(&app.state);
        let Row::List(_, rows, _) = &groups[1].rows[0] else { panic!("input list") };
        assert_eq!(rows[0], vec!["▸ Keys".to_string(), "not open".into(), "---".into()]);
        assert_eq!(rows[1][0], "  Pads");
    }
}
