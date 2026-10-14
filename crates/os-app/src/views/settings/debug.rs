//! Settings > Debug: the debugging log (version, project, devices, listed plugins, last message).
// spec: ui/settings.md "Debug"

use super::{g, Group, Row, SAct};
use crate::app::{App, State};
use crate::plugin_db as db;

/// The checks and buttons of this page that work (`SAct::Check` / `SAct::Button` labels, handled in `act`).
pub const LIVE: &[&str] = &[];

/// The Debug page's groups (spec §4).
pub fn page(s: &State) -> Vec<Group> {
    use Row::*;
    let st = &s.settings;
    let mut lines = vec![
        format!("Lemon Studio {}", env!("CARGO_PKG_VERSION")),
        format!("Project: {}", s.path.as_ref().map_or("(new project)".into(), |p| p.display().to_string())),
        format!("Saved by: {}", s.project.saved_by),
        format!("PPQ {}, tempo {:.3}", s.project.ppq, s.tempo),
        format!("Channels {}, patterns {}, mixer tracks {}", s.project.channels.len(), s.project.patterns.len(), s.project.mixer.len()),
        format!("Output device: {}", st.device),
        format!("Device rate: {:?}", st.rate),
        format!("Instruments listed: {}, effects listed: {}", s.plugdb.lists.gens.len(), s.plugdb.lists.fx.len()),
    ];
    lines.extend(s.plugdb.lists.gens.iter().map(|e| format!("  {} ({})", e.label, db::format_label(e.format))));
    if let Some(m) = &s.message {
        lines.push(format!("Last message: {m}"));
    }
    vec![g("", vec![List(vec![("", 0.0)], lines.into_iter().map(|l| vec![l]).collect(), 40)])]
}

/// This page's working controls (`LIVE`); none yet.
pub(crate) fn act(_app: &mut App, _a: &SAct) -> bool {
    false
}
