//! HELP menu entries (`menu_registry` area): the help index (the project's README), what's new (its status page), the
//! diagnostics and About. Lemon Studio choice: the two documents are built into the program and opened as text files
//! with the system's viewer (written to the temporary folder first); the diagnostics show in an information box.
// spec: ui/menus.md (HELP)

use std::path::Path;

use winit::keyboard::Key;

use crate::app::App;
use crate::menu_registry::Mods;
use crate::views::menus::HELP_MENU;

/// HELP > Help index: the README.
const README: &str = include_str!("../../../README.md");
/// HELP > What's new: what works so far.
const STATUS: &str = include_str!("../../../docs/status.md");

pub fn menu_handles(menu: usize, label: &str) -> bool {
    menu == HELP_MENU && matches!(label, "Help index" | "Diagnostics" | "What's new" | "About...")
}

pub fn menu_run(app: &mut App, _menu: usize, label: &str) -> Option<String> {
    match label {
        "Help index" => Some(show_doc("README.md", README)),
        "What's new" => Some(show_doc("status.md", STATUS)),
        "Diagnostics" => {
            crate::dialog::inform("Lemon Studio diagnostics", &diagnostics(app));
            None
        }
        "About..." => Some(concat!("Lemon Studio ", env!("CARGO_PKG_VERSION"), " (pre-alpha), free software under the GNU GPL v3 or later").into()),
        _ => None,
    }
}

pub fn key(_app: &mut App, _key: &Key, _mods: Mods) -> bool {
    false
}

/// Writes document `name` to the temporary folder and opens it with the system's viewer. Returns the message.
fn show_doc(name: &str, text: &str) -> String {
    let path = std::env::temp_dir().join(format!("lemon-studio-{name}"));
    if let Err(e) = std::fs::write(&path, text) {
        return format!("Could not write {}: {e}", path.display());
    }
    if open_file(&path) {
        format!("Opened {name}")
    } else {
        format!("{name} is at {} (no program to show it)", path.display())
    }
}

/// Opens `path` with the program the system uses for it; false when none could be started.
#[cfg(not(test))]
fn open_file(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(windows)]
    let mut cmd = std::process::Command::new("explorer");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(path).spawn().is_ok()
}

#[cfg(test)]
thread_local! {
    /// Tests open nothing: the files they would have opened.
    static OPENED: std::cell::RefCell<Vec<std::path::PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
fn open_file(path: &Path) -> bool {
    OPENED.with_borrow_mut(|o| o.push(path.to_path_buf()));
    true
}

/// The diagnostics: versions, system, audio device, plugins, project, user folder, log.
pub fn diagnostics(app: &App) -> String {
    let s = &app.state;
    let tp = &app.transport;
    let audio = match tp.opened() {
        Some((dev, rate, buf)) => format!("{dev}, {rate} Hz{}", buf.map_or_else(String::new, |b| format!(", buffer {b} samples"))),
        None => "not opened yet".into(),
    };
    let log = crate::log::entries();
    let errors = log.iter().filter(|e| e.error).count();
    let lines = [
        format!("Lemon Studio {} (pre-alpha)", env!("CARGO_PKG_VERSION")),
        format!("System: {} {}", std::env::consts::OS, std::env::consts::ARCH),
        format!("Audio output: {audio}"),
        format!("Dropouts: {}", tp.underruns()),
        format!("MIDI inputs open: {}", if s.midi_inputs.is_empty() { "none".to_string() } else { s.midi_inputs.join(", ") }),
        format!("Plugins listed: {} instruments, {} effects", s.plugdb.lists.gens.len(), s.plugdb.lists.fx.len()),
        format!("Plugins loaded: {} instances", app.plugins.hosted.len()),
        format!("Project: {}", s.path.as_ref().map_or("(not saved yet)".into(), |p| p.display().to_string())),
        format!(
            "  {} channels, {} patterns, {} mixer tracks, PPQ {}, tempo {:.3}",
            s.project.channels.len(),
            s.project.patterns.len(),
            s.project.mixer.len(),
            s.project.ppq,
            s.tempo
        ),
        format!("User folder: {}", os_paths::user_dir().map_or("(none)".into(), |d| d.display().to_string())),
        format!("Log: {} entries, {errors} errors (OPTIONS > Debugging log)", log.len()),
    ];
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::menus::item_at;

    fn index(label: &str) -> usize {
        (0..20).find(|&i| item_at(HELP_MENU, i).is_some_and(|it| it.label == label)).unwrap()
    }

    /// Help index and What's new open the built-in README and status page; Diagnostics shows the readings.
    #[test]
    fn help_entries_open_documents() {
        let mut app = App::new(None);
        for label in ["Help index", "What's new", "Diagnostics"] {
            assert!(crate::views::menus::works(HELP_MENU, item_at(HELP_MENU, index(label)).unwrap()), "{label} works");
        }
        app.menu_item(HELP_MENU, index("Help index"));
        assert_eq!(app.state.message.as_deref(), Some("Opened README.md"));
        app.menu_item(HELP_MENU, index("What's new"));
        assert_eq!(app.state.message.as_deref(), Some("Opened status.md"));
        let opened = OPENED.with_borrow(|o| o.clone());
        assert_eq!(opened.len(), 2);
        assert_eq!(std::fs::read_to_string(&opened[0]).unwrap(), README);
        assert!(README.contains("Lemon Studio") && !STATUS.is_empty());
        app.menu_item(HELP_MENU, index("Diagnostics"));
        let told = crate::dialog::fake::informed();
        let d = told.last().expect("Diagnostics shows an information box");
        assert!(d.starts_with("Lemon Studio diagnostics: Lemon Studio "));
        assert!(d.contains("Audio output:") && d.contains("Plugins listed:") && d.contains("channels"));
        // Tutorials has no Lemon Studio meaning yet.
        assert!(!crate::views::menus::works(HELP_MENU, item_at(HELP_MENU, index("Tutorials")).unwrap()));
    }

    /// The toolbar's Help button opens the help index.
    #[test]
    fn toolbar_help_button() {
        let Ok(mut c) = os_ui::Canvas::new(1920, 1080, 1.0) else { return };
        let mut app = App::new(None);
        app.paint(&mut c, None);
        let h = app.hits.iter().find(|h| h.hint == "Help index").expect("the help button");
        assert_eq!(h.action, crate::app::Action::MenuItem(HELP_MENU, index("Help index")));
    }
}
