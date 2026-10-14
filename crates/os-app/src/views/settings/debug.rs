//! Settings > Debug: the debugging log (`log.rs`: the messages of this run, newest first, errors marked) with its Clear
//! button, then the readings (version, project, devices, listed plugins).
// spec: ui/settings.md "Debug"

use super::{g, Group, Row, SAct};
use crate::app::{App, State};
use crate::plugin_db as db;

/// The checks and buttons of this page that work (`SAct::Check` / `SAct::Button` labels, handled in `act`).
pub const LIVE: &[&str] = &["Clear log"];

/// Log lines shown at once (the page scrolls past them).
const LOG_ROWS: usize = 24;

/// The log's rows, newest first: time, "!" for an error, the message.
pub fn log_rows() -> Vec<Vec<String>> {
    let entries = crate::log::entries();
    if entries.is_empty() {
        return vec![vec![String::new(), String::new(), "Nothing logged yet".into()]];
    }
    entries.into_iter().rev().map(|e| vec![e.at, if e.error { "!".into() } else { String::new() }, e.text]).collect()
}

/// The Debug page's groups (spec §4).
pub fn page(s: &State) -> Vec<Group> {
    use Row::*;
    let st = &s.settings;
    let log = log_rows();
    let n = log.len().min(LOG_ROWS);
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
    vec![
        g("Log", vec![Buttons(vec![("Clear log", 90.0)]), List(vec![("Time", 0.0), ("", 62.0), ("Message", 74.0)], log, n)]),
        g("System", vec![List(vec![("", 0.0)], lines.into_iter().map(|l| vec![l]).collect(), 40)]),
    ]
}

/// This page's working controls (`LIVE`).
pub(crate) fn act(app: &mut App, a: &SAct) -> bool {
    match a {
        SAct::Button("Clear log") => {
            crate::log::clear();
            app.invalidate();
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Action;

    /// OPTIONS > Debugging log shows the run's messages, newest first; Clear log empties it.
    #[test]
    fn debugging_log_page() {
        let Ok(mut c) = os_ui::Canvas::new(1920, 1080, 1.0) else { return };
        let mut app = App::new(None);
        crate::log::clear();
        app.state.message = Some("Could not open x.flp: missing".into());
        crate::log::watch(&mut app);
        app.state.message = Some("Metronome on".into());
        crate::log::watch(&mut app);
        let rows = log_rows();
        assert_eq!((rows[0][2].as_str(), rows[0][1].as_str()), ("Metronome on", ""));
        assert_eq!((rows[1][2].as_str(), rows[1][1].as_str()), ("Could not open x.flp: missing", "!"));
        let i = (0..60).find(|&i| crate::views::menus::item_at(crate::views::menus::OPTIONS_MENU, i).is_some_and(|it| it.label == "Debugging log")).unwrap();
        app.menu_item(crate::views::menus::OPTIONS_MENU, i);
        app.paint(&mut c, None);
        let clear = app.hits.iter().find(|h| h.action == Action::Settings(SAct::Button("Clear log"))).expect("Clear log works").rect;
        app.mouse_move(clear.cx(), clear.cy());
        app.mouse_down();
        app.mouse_up();
        assert!(crate::log::entries().is_empty());
        assert_eq!(log_rows()[0][2], "Nothing logged yet");
    }
}
