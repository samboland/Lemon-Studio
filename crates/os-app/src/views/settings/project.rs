//! Settings > Project: data folder, time settings, advanced, playlist.
// spec: ui/settings.md "Project"

use super::{combo, g, gf, txt, Group, Row, SAct};
use crate::app::{App, State};

/// The checks and buttons of this page that work (`SAct::Check` / `SAct::Button` labels, handled in `act`).
pub const LIVE: &[&str] = &[];

/// The Project page's groups (spec §4).
pub fn page(s: &State) -> Vec<Group> {
    use Row::*;
    vec![
        g("Data folder", vec![
            combo("Don't use a data folder", 306.0, ""),
            txt("(none)"),
            combo("When saving unsaved projects only", 444.0, "Ask for data folder"),
            Off("Copy used samples to the project folder", false),
        ]),
        g("Time settings", vec![
            combo(&s.project.ppq.to_string(), 44.0, "Timebase (PPQ)"),
            combo("Set as time signature", 196.0, ""),
            combo("4", 44.0, "Numerator"),
            combo("4", 44.0, "Denominator"),
        ]),
        gf("Advanced", true, vec![
            combo("Circular", 82.0, "Panning law"),
            Check("Play truncated notes in clips", true),
            Check("Fast declick for cut groups", false),
        ]),
        gf("Playlist", false, vec![Check2("Automatic crossfades", true, "Enable automatic crossfades in new projects", true)]),
    ]
}

/// This page's working controls (`LIVE`); none yet.
pub(crate) fn act(_app: &mut App, _a: &SAct) -> bool {
    false
}
