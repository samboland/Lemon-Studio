//! Settings > File: backup, plugins (counts and the plugin manager), the user data folder, browser folders, external
//! tools, troubleshooting.
// spec: ui/settings.md "File"

use super::{combo, dim, g, gf, pair, txt, Group, Row, SAct, PLUGINS};
use crate::app::{App, State};

/// The checks and buttons of this page that work (`SAct::Check` / `SAct::Button` labels, handled in `act`).
pub const LIVE: &[&str] = &["Manage plugins"];

/// The File page's groups (spec §4).
pub fn page(s: &State) -> Vec<Group> {
    use Row::*;
    let user = os_paths::user_dir().map(|d| format!("{}{}", d.display(), std::path::MAIN_SEPARATOR)).unwrap_or_default();
    vec![
        g("Backup", vec![
            pair(combo("Occasionally (every 10 minutes)", 336.0, "Autosave"), ValueR("Maximum", "100".into())),
            combo("Save to the project data folder if there is one", 336.0, "Location"),
        ]),
        g("Plugins", vec![
            Buttons(vec![("Manage plugins", 124.0)]),
            Text(format!("{} instruments, {} effects", s.plugdb.lists.gens.len(), s.plugdb.lists.fx.len()), true),
        ]),
        g("User data folder", vec![txt(user)]),
        g("Browser extra search folders", vec![dim("Browser extra search folders", List(vec![("Folder", 30.0), ("Browser name", 440.0)], vec![], 12))]),
        g("External tools", vec![
            dim("External tools", List(vec![("File", 30.0), ("Name", 320.0)], vec![], 4)),
            dim("External tools", txt("Script editor   C:\\Windows\\System32\\Notepad.exe")),
        ]),
        gf("Troubleshooting", true, vec![Buttons(vec![("Repair optional features", 124.0), ("Reset browser cache", 124.0)])]),
    ]
}

/// This page's working controls (`LIVE`).
pub(crate) fn act(app: &mut App, a: &SAct) -> bool {
    match *a {
        SAct::Button("Manage plugins") => app.open_settings(PLUGINS),
        _ => return false,
    }
    true
}
