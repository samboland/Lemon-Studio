//! FILE menu entries (`menu_registry` area): new, open, save, revert to the last backup (`autosave`), export, exit.
// spec: ui/menus.md (FILE)

use winit::keyboard::Key;

use crate::app::App;
use crate::menu_registry::Mods;
use crate::views::menus::FILE_MENU;

pub fn menu_handles(menu: usize, label: &str) -> bool {
    menu == FILE_MENU && matches!(label, "New" | "Open..." | "Save" | "Save as..." | "Save new version" | "Revert to last backup" | "Export as WAV..." | "Exit")
}

pub fn menu_run(app: &mut App, _menu: usize, label: &str) -> Option<String> {
    match label {
        "New" => {
            if !app.confirm_discard("starting a new project") {
                return None;
            }
            app.transport.stop();
            app.reset_hosted();
            let s = &mut app.state;
            s.project = crate::app::default_project();
            s.file = None;
            s.path = None;
            s.tempo = 130.0;
            s.pattern = 1;
            s.piano.reset();
            s.playlist.reset();
            crate::playlist::load_view(s);
            s.rack.reset();
            crate::rack::from_model(s);
            s.piano.dirty = true;
            s.message = Some("New project".into());
            app.mark_saved();
        }
        "Open..." => {
            if !app.confirm_discard("opening another project") {
                return None;
            }
            let start = app.state.path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf()));
            let mut dlg = rfd::FileDialog::new().add_filter("FLP project", &["flp"]);
            if let Some(d) = start {
                dlg = dlg.set_directory(d);
            }
            if let Some(p) = crate::dialog::pick_file(dlg) {
                app.open(p);
            }
        }
        "Save" => app.save(false),
        "Save as..." => app.save(true),
        "Save new version" => app.save_new_version(),
        "Revert to last backup" => crate::autosave::revert(app),
        "Export as WAV..." => app.export_wav(),
        "Exit" => app.want_close = true,
        _ => {}
    }
    None
}

/// FILE shortcuts of their own (the ones in `App::key`'s table stay there).
pub fn key(_app: &mut App, _key: &Key, _mods: Mods) -> bool {
    false
}
