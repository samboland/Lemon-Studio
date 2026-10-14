//! Settings > File: backup (autosave, `autosave.rs`), plugins (counts and the plugin manager), the user data folder,
//! browser folders, external tools, troubleshooting.
// spec: ui/settings.md "File"

use super::{combo, dim, g, gf, pair, txt, Group, Plug, Row, SAct, FILE, PLUGINS};
use crate::app::{App, State};
use crate::app_settings::{self as cfg, AUTOSAVE};

/// Adds a browser extra search folder (`browser_folders.txt`, the folders dropped on the window).
pub const ADD_FOLDER: &str = "Add browser folder...";

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
    ADD_FOLDER,
];

/// The File page's groups (spec §4).
pub fn page(s: &State) -> Vec<Group> {
    use Row::*;
    let user = os_paths::user_dir().map(|d| format!("{}{}", d.display(), std::path::MAIN_SEPARATOR)).unwrap_or_default();
    let mins = cfg::get().autosave;
    let on = |i: usize| AUTOSAVE[i].0 == mins;
    // Browser extra folders (the All tab's roots): each can be removed, none switched off (always listed).
    let folders = s.browser_folders.iter().map(|f| (f.display().to_string(), "", true, true)).collect();
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
        g("Browser extra search folders", vec![Folders(folders), Buttons(vec![(ADD_FOLDER, 160.0)])]),
        g("External tools", vec![
            dim("External tools", List(vec![("File", 30.0), ("Name", 320.0)], vec![], 4)),
            dim("External tools", txt("Script editor   C:\\Windows\\System32\\Notepad.exe")),
        ]),
        gf("Troubleshooting", true, vec![Buttons(vec![("Repair optional features", 124.0), ("Reset browser cache", 124.0)])]),
    ]
}

/// This page's working controls (`LIVE`), and the browser folder list (`SAct::Plug` while this page shows).
pub(crate) fn act(app: &mut App, a: &SAct) -> bool {
    let here = app.state.settings.tab == FILE;
    match *a {
        SAct::Button("Manage plugins") => app.open_settings(PLUGINS),
        SAct::Check(k) if AUTOSAVE.iter().any(|a| a.1 == k) => {
            let mins = AUTOSAVE.iter().find(|a| a.1 == k).map_or(0, |a| a.0);
            cfg::set(|g| g.autosave = mins);
        }
        SAct::Button(ADD_FOLDER) => {
            let dlg = rfd::FileDialog::new().set_title("Add a browser folder");
            if let Some(p) = crate::dialog::pick_folder(dlg) {
                app.add_browser_folder(p);
            }
        }
        SAct::Plug(Plug::Remove(i)) if here => {
            let s = &mut app.state;
            if i < s.browser_folders.len() {
                let gone = s.browser_folders.remove(i);
                crate::browser::save_folders(&s.browser_folders);
                s.message = Some(format!("Removed {} from the browser", gone.display()));
            }
        }
        SAct::Plug(Plug::Folder(i)) if here => {
            if let Some(f) = app.state.browser_folders.get(i) {
                app.state.message = Some(format!("{} is listed in the browser (× removes it)", f.display()));
            }
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Action;
    use os_ui::Canvas;

    /// Autosave's choices and the browser folder list work on the File page; Location stays greyed.
    #[test]
    fn autosave_choice_and_browser_folders() {
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
        assert!(has(&app, Action::Settings(SAct::Button(ADD_FOLDER))));
        assert!(has(&app, Action::Placeholder("Location")), "the backup location stays greyed");
        app.act(Action::Settings(SAct::Check(AUTOSAVE[2].1)));
        assert_eq!(cfg::get().autosave, 5);
        cfg::forget();
        let _app = App::new(None);
        assert_eq!(cfg::get().autosave, 5, "kept in general.txt");

        // Add a browser folder (the folder dialog answers), then remove it with its ×.
        let folder = dir.join("Loops");
        std::fs::create_dir_all(&folder).unwrap();
        crate::dialog::fake::answer(Some(folder.clone()));
        app.act(Action::Settings(SAct::Button(ADD_FOLDER)));
        assert_eq!(app.state.browser_folders, [folder.clone()]);
        app.open_settings(FILE);
        app.paint(&mut c, None);
        assert!(has(&app, Action::Settings(SAct::Plug(Plug::Remove(0)))), "the folder lists with its ×");
        app.act(Action::Settings(SAct::Plug(Plug::Remove(0))));
        assert!(app.state.browser_folders.is_empty());
        assert!(app.state.message.as_deref().unwrap().starts_with("Removed"));

        cfg::set(|g| g.autosave = 10);
        cfg::test_dir(None);
        cfg::forget();
        let _ = std::fs::remove_dir_all(dir);
    }
}
