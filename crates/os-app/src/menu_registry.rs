//! Main-menu entries and shortcuts by area. Each area module says which entries it handles (`menu_handles`) and what
//! choosing one does (`menu_run`, a message to show if any); `App::menu_item` runs the one area that handles an entry
//! and `views::menus::works` lights the entries some area handles. A new working entry is a label in its area's
//! `menu_handles` and an arm in its `menu_run`: neither `app.rs` nor `views/menus.rs` changes. A new area is one line in
//! `AREAS` (and, for shortcuts, one in `KEYS`).
//!
//! Entries are matched by menu index and label (the same label can mean different things in two menus, e.g. "Delete
//! unused files and save project" in FILE and TOOLS); `tests::areas_are_disjoint` checks that no entry has two owners.
// spec: ui/menus.md, ui/main-window.md §3

use winit::keyboard::Key;

use crate::app::App;

/// One area's main-menu entries.
pub struct Area {
    /// For messages (tests).
    #[cfg_attr(not(test), allow(dead_code))]
    pub name: &'static str,
    /// Whether the area handles entry `label` of menu `menu` (`views::menus::MENUS` index).
    pub handles: fn(usize, &str) -> bool,
    /// Choose entry `label` of menu `menu`; returns a message for the hint bar, if any.
    pub run: fn(&mut App, usize, &str) -> Option<String>,
}

/// The areas, one per main menu, then the modules that bring entries of their own into several menus.
pub const AREAS: &[Area] = &[
    Area { name: "file", handles: crate::menu_file::menu_handles, run: crate::menu_file::menu_run },
    Area { name: "edit", handles: crate::menu_edit::menu_handles, run: crate::menu_edit::menu_run },
    Area { name: "add", handles: crate::menu_add::menu_handles, run: crate::menu_add::menu_run },
    Area { name: "view", handles: crate::menu_view::menu_handles, run: crate::menu_view::menu_run },
    Area { name: "options", handles: crate::menu_options::menu_handles, run: crate::menu_options::menu_run },
    Area { name: "tools", handles: crate::menu_tools::menu_handles, run: crate::menu_tools::menu_run },
    Area { name: "help", handles: crate::menu_help::menu_handles, run: crate::menu_help::menu_run },
    // PATTERNS commands and ADD > Pattern (spec: ui/pattern-menu.md).
    Area { name: "patterns", handles: crate::patterns::menu_handles, run: crate::patterns::menu_run },
    // Last tweaked parameter entries in ADD and TOOLS (spec: plugins/plugin-parameters.md §5).
    Area { name: "params", handles: params_handles, run: params_run },
];

fn params_handles(_menu: usize, label: &str) -> bool {
    crate::params::menu_handles(label)
}
fn params_run(app: &mut App, _menu: usize, label: &str) -> Option<String> {
    crate::params::menu_run(app, label)
}

/// The area that handles entry `label` of menu `menu`.
pub fn area(menu: usize, label: &str) -> Option<&'static Area> {
    AREAS.iter().find(|a| (a.handles)(menu, label))
}

/// Whether some area handles entry `label` of menu `menu` (it works in this build).
pub fn handles(menu: usize, label: &str) -> bool {
    area(menu, label).is_some()
}

/// Choose entry `label` of menu `menu`: its area runs it and its message is shown. Returns false when no area handles
/// it.
pub fn run(app: &mut App, menu: usize, label: &str) -> bool {
    let Some(a) = area(menu, label) else { return false };
    if let Some(m) = (a.run)(app, menu, label) {
        app.state.message = Some(m);
    }
    true
}

/// Modifier keys held with a shortcut.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// The areas' shortcuts, asked in order before `App::key`'s built-in table (after the keys of the window in front);
/// the first that takes the key wins.
pub const KEYS: &[fn(&mut App, &Key, Mods) -> bool] = &[
    crate::menu_file::key,
    crate::menu_edit::key,
    crate::menu_add::key,
    crate::menu_view::key,
    crate::menu_options::key,
    crate::menu_tools::key,
    crate::menu_help::key,
];

/// Offer `key` to the areas' shortcuts; returns whether one took it.
pub fn key(app: &mut App, key: &Key, mods: Mods) -> bool {
    KEYS.iter().any(|f| f(app, key, mods))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::menus::{item_at, Kind, MENUS};

    /// Every label of every menu, asked of every menu.
    fn all_labels() -> Vec<&'static str> {
        let mut v: Vec<&str> = MENUS.iter().flat_map(|m| m.cols.iter().flat_map(|(_, items)| items.iter())).filter(|it| it.kind == Kind::Item).map(|it| it.label).collect();
        v.sort();
        v.dedup();
        v
    }

    /// No entry is handled by two areas: the `menu_handles` sets are disjoint.
    #[test]
    fn areas_are_disjoint() {
        for menu in 0..MENUS.len() {
            for label in all_labels() {
                let owners: Vec<&str> = AREAS.iter().filter(|a| (a.handles)(menu, label)).map(|a| a.name).collect();
                assert!(owners.len() <= 1, "{} > {label}: handled by {owners:?}", MENUS[menu].title);
            }
        }
    }

    /// Every entry that worked before the registry still works (the list is the build before it; entries may only be
    /// added to the working set).
    #[test]
    fn working_entries_still_work() {
        const WORKED: &[(usize, &str)] = &[
            // FILE
            (0, "New"), (0, "Open..."), (0, "Save"), (0, "Save as..."), (0, "Save new version"),
            (0, "Export as WAV..."), (0, "Exit"),
            // EDIT
            (1, "Undo"),
            // ADD
            (2, "More plugins..."), (2, "Sampler"), (2, "Plugin picker"), (2, "Refresh plugin list (fast scan)"),
            (2, "Manage plugins"), (2, "Automation for last tweaked parameter"), (2, "Pattern"),
            // PATTERNS
            (3, "Find first empty..."), (3, "Find next empty..."), (3, "Find next empty (no naming)"),
            (3, "Select in playlist"), (3, "Rename and color..."), (3, "Change color..."), (3, "Random color"),
            (3, "Transpose..."), (3, "Insert one"), (3, "Clone"), (3, "Delete"), (3, "Move up"), (3, "Move down"),
            // VIEW
            (4, "Playlist"), (4, "Piano roll"), (4, "Channel rack"), (4, "Mixer"), (4, "Browser"),
            (4, "Plugin picker"), (4, "Tempo tapper"), (4, "Plugin performance monitor"), (4, "Close all windows"),
            (4, "Close all plugin windows"), (4, "Close all unfocused windows"),
            // OPTIONS
            (5, "MIDI settings"), (5, "Audio settings"), (5, "General settings"), (5, "File settings"),
            (5, "Theme settings"), (5, "Manage plugins"), (5, "Project general settings"), (5, "Project info"),
            (5, "Debugging log"), (5, "Typing keyboard to piano"), (5, "Metronome"), (5, "Auto scrolling"),
            (5, "Enable groups"),
            // TOOLS
            (6, "Last tweaked"), (6, "Next to last tweaked"), (6, "Stop sound"), (6, "Select unused channels"),
            (6, "Purge unused audio clips"), (6, "Reset empty playlist tracks"),
            // HELP
            (7, "About..."),
        ];
        for &(menu, label) in WORKED {
            let item = (0..200).filter_map(|i| item_at(menu, i)).find(|it| it.label == label && it.kind == Kind::Item);
            let item = item.unwrap_or_else(|| panic!("{} > {label}: no such entry", MENUS[menu].title));
            assert!(crate::views::menus::works(menu, item), "{} > {label} no longer works", MENUS[menu].title);
        }
    }

    /// Choosing an entry no area handles only says it is not available yet.
    #[test]
    fn unhandled_entries_are_not_available() {
        let mut app = App::new(None);
        let i = (0..200).find(|&i| item_at(crate::views::menus::HELP_MENU, i).is_some_and(|it| it.label == "Help index")).expect("Help index");
        assert!(!handles(crate::views::menus::HELP_MENU, "Help index"));
        app.menu_item(crate::views::menus::HELP_MENU, i);
        assert_eq!(app.state.message.as_deref(), Some("Help index: not available yet"));
    }
}
