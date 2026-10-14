//! Settings > File: backup (autosave, `autosave.rs`), plugins (counts and the plugin manager), the user data folder,
//! browser folders, external tools, troubleshooting.
// spec: ui/settings.md "File"

use super::{combo, dim, g, gf, pair, txt, Group, Row, SAct, PLUGINS};
use crate::app::{App, State};
use crate::app_settings::{self as cfg, AUTOSAVE};

/// The checks and buttons of this page that work (`SAct::Check` / `SAct::Button` labels, handled in `act`). The
/// Autosave combo's choices are radio checks (`app_settings::AUTOSAVE`), as settings rows have no menus of their own
/// yet (Lemon Studio choice).
pub const LIVE: &[&str] = &[
    "Manage plugins",
    AUTOSAVE[0].1,
    AUTOSAVE[1].1,
    AUTOSAVE[2].1,
    AUTOSAVE[3].1,
    AUTOSAVE[4].1,
];

/// The File page's groups (spec §4).
pub fn page(s: &State) -> Vec<Group> {
    use Row::*;
    let user = os_paths::user_dir().map(|d| format!("{}{}", d.display(), std::path::MAIN_SEPARATOR)).unwrap_or_default();
    let mins = cfg::get().autosave;
    let on = |i: usize| AUTOSAVE[i].0 == mins;
    vec![
        g("Backup", vec![
            Text("Autosave a changed project to the Backup folder of the user data folder:".into(), true),
            Check2(AUTOSAVE[0].1, on(0), AUTOSAVE[1].1, on(1)),
            Check2(AUTOSAVE[2].1, on(2), AUTOSAVE[3].1, on(3)),
            pair(Check(AUTOSAVE[4].1, on(4)), ValueR("Maximum", crate::autosave::MAX_BACKUPS.to_string())),
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
        SAct::Check(k) if AUTOSAVE.iter().any(|a| a.1 == k) => {
            let mins = AUTOSAVE.iter().find(|a| a.1 == k).map_or(0, |a| a.0);
            cfg::set(|g| g.autosave = mins);
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Action;
    use crate::views::settings::FILE;
    use os_ui::Canvas;

    /// Autosave's choices work on the File page; Location stays greyed.
    #[test]
    fn autosave_choice() {
        let Ok(mut c) = Canvas::new(1920, 1080, 1.0) else { return };
        let dir = std::env::temp_dir().join(format!("lemon-file-page-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        cfg::test_dir(Some(dir.clone()));
        cfg::forget();
        let mut app = App::new(None);
        app.open_settings(FILE);
        app.paint(&mut c, None);
        let has = |app: &App, a: Action| app.hits.iter().any(|h| h.action == a);
        for (_, k) in AUTOSAVE {
            assert!(has(&app, Action::Settings(SAct::Check(k))) && !has(&app, Action::Placeholder(k)), "{k} works");
        }
        assert!(has(&app, Action::Placeholder("Location")), "the backup location stays greyed");
        app.act(Action::Settings(SAct::Check(AUTOSAVE[2].1)));
        assert_eq!(cfg::get().autosave, 5);
        cfg::forget();
        let _app = App::new(None);
        assert_eq!(cfg::get().autosave, 5, "kept in general.txt");
        cfg::set(|g| g.autosave = 10);
        cfg::test_dir(None);
        cfg::forget();
        let _ = std::fs::remove_dir_all(dir);
    }
}
