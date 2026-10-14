//! Settings > General: display, language, input, undo history, hints, system, updates, miscellaneous, advanced.
// spec: ui/settings.md "General"

use super::{combo, dim, gf, txt, Group, Row, SAct};
use crate::app::{App, State};

/// The checks and buttons of this page that work (`SAct::Check` / `SAct::Button` labels, handled in `act`).
pub const LIVE: &[&str] = &[];

/// The General page's groups (spec §4).
pub fn page(_s: &State) -> Vec<Group> {
    use Row::*;
    vec![
        Group { note: " (requires a restart to take effect)", ..gf("Display", false, vec![
            combo("System", 64.0, "Main GUI scaling"),
            dim("Display PPI", txt("Define 1 inch / 2.54 cm or set display's PPI")),
            dim("Display PPI", Text("(96 PPI/advised scaling: 100%)".into(), true)),
            Slider("", 0.25),
            combo("Main", 64.0, "Pop-ups scaling"),
            combo("Main", 64.0, "Toolbars scaling"),
            combo("Off", 64.0, "Transparent windows"),
            combo("Entertain me", 126.0, "Animations"),
            Slider("Smoothing", 0.2),
            combo("Ultrasmooth", 126.0, "Animation refresh rate"),
            Check2("Force refreshes", true, "Transparent menus", true),
            Off("Thick lines", true),
            Check("High visibility", false),
            combo("Off", 146.0, "Colorful waveforms"),
        ]) },
        Group { note: " (requires a restart to take effect)", ..gf("Language", false, vec![
            combo("English (US) (en)", 176.0, "Language"),
            combo("English", 176.0, "Note names"),
            combo("Sharps - #", 96.0, "Black notes"),
            Buttons(vec![("Update languages", 138.0)]),
        ]) },
        gf("Input", false, vec![
            combo("Multi-button mouse", 136.0, "Optimize for"),
            Buttons(vec![("Manage shortcuts", 212.0)]),
            Check("Click-and-hold & special gesture functions", false),
            Check("Use both mouse buttons in step sequencer", false),
            Slider("Vertical scrolling speed", 0.5),
            Slider("Horizontal scrolling speed", 0.5),
            Check("Invert pencil buttons", false),
        ]),
        gf("Undo history", false, vec![
            Value("100".into(), "Maximum undo levels"),
            Check("Undo knob tweaks", true),
            Check("Alternate undo mode", true),
            Check("Put undone recordings in the recycle bin", true),
            Check("Show undo history in the browser when undoing changes", false),
            Check("Show recent changes at the top", false),
        ]),
        gf("Hints and warnings", false, vec![
            Buttons(vec![("Manage messages", 138.0)]),
            combo("100MB (default)", 110.0, "Project data size warning"),
        ]),
        gf("System", false, vec![Check("Associate project files with application", true)]),
        gf("Updates and privacy", false, vec![
            Check("Automatically check for updates", true),
            Check("Automatically download program updates", true),
            Check("Send usage data", false),
        ]),
        gf("Miscellaneous", false, vec![
            Check("Auto name channels", true),
            Check("Auto name effect slots", false),
            Check("Auto zip empty channels", false),
            Check("Auto select linked modules", true),
            Check("Auto zoom in piano roll", false),
            Check("Small scrollbars in editors", false),
            Check2("Detach all new plugins", false, "Open detached plugins with the main window", false),
            Check("Disable typing keyboard priority by default", false),
            Check("Alternate meter scale", false),
            Check("Show startup splash screen", true),
            Check("Restore previous state after solo", false),
            Check("Hide plugin window toolbar by default", true),
            Check("Silent startup", false),
            Check("Monophonic painting in piano roll", false),
            Check("Create automation clips at the playback position", true),
            Check("Select first channel with notes when the pattern changes", false),
            Check("Limit the number of open plugin editors", true),
            combo("Basic with limiter", 160.0, "Default template"),
            combo("Default template", 160.0, "Startup project"),
        ]),
        gf("Advanced", false, vec![
            Check("Don't limit windows to screen", false),
            Check("Fast sample preview", true),
            Check("Auto keep long audio on disk", true),
            Check2("Read sample tempo information", true, "Read sample root note", true),
            Check("Initialize controls automatically", true),
            Check("Force high performance power plan", true),
            Check("Threaded project sample loading", false),
            combo("System", 64.0, "Cursors scaling"),
            combo("Main", 64.0, "Legacy scaling"),
            combo("Show help in a web browser", 160.0, ""),
        ]),
    ]
}

/// This page's working controls (`LIVE`); none yet.
pub(crate) fn act(_app: &mut App, _a: &SAct) -> bool {
    false
}
