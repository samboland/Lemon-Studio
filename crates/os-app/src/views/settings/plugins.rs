//! Settings > Plugins: the plugin manager (search folders, scanning, the plugin table) and the experimental native host
//! switches.
// spec: ui/plugin-manager.md §2, plugins/fruity-abi.md

use super::{g, Group, Row, SAct};
use crate::app::{App, State};
use crate::plugin_db as db;

/// Opt in to the experimental FL native plugin host (spec plugins/fruity-abi.md), on the Plugins page only with
/// `OPENSTUDIO_NATIVE_SETTINGS` (`SettingsUi::native`). Labels are the keys for `SAct::Check`; `act` acts on
/// them.
pub const NATIVE_HOST_CHECK: &str = "Experimental: host installed native plugins";
/// Shown only while the native host is on: an opened project's native plugins are played by the installed plugins,
/// never by the built-in Lemon recreations (spec plugins/fruity-abi.md "Opened projects").
pub const NATIVE_FIRST_CHECK: &str = "Experimental: real native plugins instead of Lemon recreations";
/// Plugin manager checks (spec: ui/plugin-manager.md §2).
pub const STARTUP_CHECK: &str = "Find new plugins at startup";
pub const ERRORS_CHECK: &str = "Rescan plugins with errors";
pub const COMBINE_CHECK: &str = "Combine formats of the same plugin";

/// Whether `key` is one of the plugin manager's own checks (handled by `App::plugin_check`).
pub fn plugin_check(key: &str) -> bool {
    [STARTUP_CHECK, ERRORS_CHECK, COMBINE_CHECK].contains(&key)
}

/// The checks and buttons of this page that work (`SAct::Check` / `SAct::Button` labels, handled in `act`).
pub const LIVE: &[&str] = &[
    NATIVE_HOST_CHECK,
    NATIVE_FIRST_CHECK,
    STARTUP_CHECK,
    ERRORS_CHECK,
    COMBINE_CHECK,
    "Add folder...",
    "Find new plugins",
    "Rescan all",
];

/// The Plugins page's groups (spec: ui/plugin-manager.md §2).
pub fn page(s: &State) -> Vec<Group> {
    use Row::*;
    let st = &s.settings;
    let d = &s.plugdb;
    let p = &d.prefs;
    let folders = db::search_dirs(p).into_iter().map(|f| (f.path.display().to_string(), f.tag(), f.on, f.kind == db::DirKind::Added)).collect();
    let mut scanning = vec![
        Check(STARTUP_CHECK, p.startup_scan),
        Check(ERRORS_CHECK, p.rescan_errors),
        Check(COMBINE_CHECK, p.combine),
        Prefer(p.prefer),
    ];
    // The native host toggle appears only with `OPENSTUDIO_NATIVE_SETTINGS`, the second one only while the
    // native host is on too.
    if st.native {
        scanning.push(Check(NATIVE_HOST_CHECK, os_native::enabled()));
        if os_native::enabled() {
            scanning.push(Check(NATIVE_FIRST_CHECK, os_native::prefer_native()));
        }
    }
    scanning.push(Buttons(vec![("Find new plugins", 130.0), ("Rescan all", 100.0)]));
    scanning.push(Text(d.status_line(), true));
    vec![
        g("Search folders", vec![Folders(folders), Buttons(vec![("Add folder...", 110.0)])]),
        g("Scanning", scanning),
        g("Plugins", vec![Table]),
    ]
}

/// This page's working controls (`LIVE`) and the plugin manager's (`SAct::Plug`).
pub(crate) fn act(app: &mut App, a: &SAct) -> bool {
    match *a {
        // Opt in / out of the experimental native host and scan again so the effect lists follow (spec
        // plugins/fruity-abi.md). Its displayed state comes straight from `os_native::enabled()`.
        SAct::Check(k) if k == NATIVE_HOST_CHECK => {
            let on = !os_native::enabled();
            os_native::set_enabled(on);
            app.start_scan(crate::plugin_db::Mode::Changed);
            app.state.message = Some(if on { "Experimental native plugin host on: looking for native plugins".into() } else { "Experimental native plugin host off".into() });
            // The hidden preference applies again (or stops applying) with the host.
            if os_native::prefer_native_set() {
                app.swap_builtin_overrides();
            }
        }
        // Real native plugins in place of the Lemon recreations (spec plugins/fruity-abi.md "Opened projects").
        SAct::Check(k) if k == NATIVE_FIRST_CHECK => {
            let on = !os_native::prefer_native();
            os_native::set_prefer_native(on);
            let n = app.swap_builtin_overrides();
            app.state.message = Some(match (on, n) {
                (true, 0) => "Opened projects play their own native plugins, not the Lemon recreations".into(),
                (true, n) => format!("Opened projects play their own native plugins: {n} channels / slots switched from the Lemon recreations"),
                (false, 0) => "Lemon recreations play their projects' plugins again".into(),
                (false, n) => format!("Lemon recreations play their projects' plugins again: {n} channels / slots switched back"),
            });
        }
        // The plugin manager (spec: ui/plugin-manager.md §2).
        SAct::Check(k) if plugin_check(k) => app.plugin_check(k),
        SAct::Plug(p) => app.plugin_act(p),
        SAct::Button("Find new plugins") => app.start_scan(crate::plugin_db::Mode::Changed),
        SAct::Button("Rescan all") => app.start_scan(crate::plugin_db::Mode::All),
        SAct::Button("Add folder...") => app.add_plugin_folder(),
        _ => return false,
    }
    true
}
