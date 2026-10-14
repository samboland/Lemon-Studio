//! Settings > MIDI: output and input devices, external sync.
// spec: ui/settings.md "MIDI", ui/midi-input.md

use super::{combo, dim, g, pair, txt, Group, Row, SAct};
use crate::app::{App, State};

/// The checks and buttons of this page that work (`SAct::Check` / `SAct::Button` labels, handled in `act`).
pub const LIVE: &[&str] = &[];

/// The MIDI page's groups (spec §4).
pub fn page(s: &State) -> Vec<Group> {
    use Row::*;
    vec![
        g("Output", vec![
            dim("MIDI output devices", List(vec![("", 0.0)], vec![], 8)),
            pair(Check2("Send master sync", false, "Send all notes off", true), ValueR("Port", "---".into())),
            pair(Value("0".into(), "Master sync offset (milliseconds)"), combo("MIDI clock", 168.0, "Synchronization type")),
        ]),
        g("Input", vec![
            // The devices that are open: their notes play the selected channel (spec: ui/midi-input.md).
            List(vec![("Open input devices (notes play the selected channel)", 0.0)], s.midi_inputs.iter().map(|d| vec![d.clone()]).collect(), 9),
            pair(Check("Enable", true), ValueR("Port", "---".into())),
            combo("(generic controller)", 250.0, "Controller type"),
            pair(combo("Velocity", 100.0, "Link note on velocity to"), ValueR("Omni preview MIDI channel", "---".into())),
            pair(combo("Release", 100.0, "Link release velocity to"), ValueR("Song marker jump MIDI channel", "---".into())),
            pair(Check("Pickup (takeover mode)", false), ValueR("Performance mode MIDI channel", "---".into())),
            pair(Check("Auto accept detected controller", true), ValueR("Generator muting MIDI channel", "---".into())),
            pair(Check("Foot pedal controls note off", true), Check("Toggle on release", false)),
        ]),
        g("External sync", vec![
            pair(combo("Off", 168.0, "External clock sync"), ValueR("External sync offset (milliseconds)", "0".into())),
            txt("Status: off"),
        ]),
        g("", vec![Buttons(vec![("Refresh device list", 136.0), ("Update MIDI scripts", 136.0)])]),
    ]
}

/// This page's working controls (`LIVE`); none yet.
pub(crate) fn act(_app: &mut App, _a: &SAct) -> bool {
    false
}
