//! The project picker (VIEW > Project picker, Ctrl+F8, the toolbar's project button): a menu of the projects opened or
//! saved lately, newest first; choosing one opens it the way FILE > Open does (asking to save unsaved changes first).
//! The list is a program setting, not part of a project: kept in the user folder (`recent.txt`, one `project=<path>`
//! line each). Lemon Studio choice: FL's picker is a window over the project's contents; Lemon's lists projects.
// spec: ui/menus.md (VIEW), ui/main-window.md §3

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::app::{App, State};
use crate::menu_view::VCmd;
use crate::playlist::{Cmd, PItem, Popup};

/// Projects kept in the list.
pub const MAX: usize = 12;

#[derive(Default)]
struct Recent {
    list: Vec<PathBuf>,
    /// The project path `watch` saw last.
    seen: Option<PathBuf>,
}

#[cfg_attr(test, allow(dead_code))]
static RECENT: OnceLock<Mutex<Recent>> = OnceLock::new();

#[cfg(test)]
thread_local! {
    /// Tests run in parallel threads: each has its own list, and nothing is written.
    static TEST: std::cell::RefCell<Recent> = std::cell::RefCell::default();
}

/// Runs `f` on the list; keeps it when `f` changed it.
fn with<R>(f: impl FnOnce(&mut Recent) -> R) -> R {
    #[cfg(test)]
    return TEST.with_borrow_mut(f);
    #[cfg(not(test))]
    {
        let mut r = RECENT.get_or_init(|| Mutex::new(Recent { list: load(), seen: None })).lock().unwrap_or_else(|e| e.into_inner());
        let before = r.list.clone();
        let out = f(&mut r);
        if r.list != before {
            save(&r.list);
        }
        out
    }
}

/// The recent projects, newest first.
pub fn recents() -> Vec<PathBuf> {
    with(|r| r.list.clone())
}

/// `path` was opened or saved: it goes to the top.
pub fn note(path: &Path) {
    with(|r| add(&mut r.list, path));
}

fn add(list: &mut Vec<PathBuf>, path: &Path) {
    list.retain(|p| p != path);
    list.insert(0, path.to_path_buf());
    list.truncate(MAX);
}

pub fn forget(path: &Path) {
    with(|r| r.list.retain(|p| p != path));
}

pub fn clear() {
    with(|r| r.list.clear());
}

/// Called every UI tick (`log::watch`): the project's path changed (opened, saved as) → it is a recent project.
pub fn watch(s: &State) {
    let Some(path) = s.path.as_ref() else { return };
    let new = with(|r| {
        let new = r.seen.as_ref() != Some(path);
        r.seen = Some(path.clone());
        new
    });
    if new {
        note(path);
    }
}

/// `project=<path>` lines, newest first; anything else is ignored.
pub fn parse(text: &str) -> Vec<PathBuf> {
    let mut list = Vec::new();
    for line in text.lines() {
        if let Some(("project", p)) = line.split_once('=').map(|(k, v)| (k.trim(), v.trim())) {
            if !p.is_empty() && !list.iter().any(|q: &PathBuf| q.as_os_str() == p) {
                list.push(PathBuf::from(p));
            }
        }
    }
    list.truncate(MAX);
    list
}

pub fn text(list: &[PathBuf]) -> String {
    list.iter().map(|p| format!("project={}\n", p.display())).collect()
}

#[cfg_attr(test, allow(dead_code))]
fn load() -> Vec<PathBuf> {
    os_paths::user_file("recent.txt").and_then(|f| std::fs::read_to_string(f).ok()).map(|t| parse(&t)).unwrap_or_default()
}

#[cfg_attr(test, allow(dead_code))]
fn save(list: &[PathBuf]) {
    if let Some(dir) = os_paths::user_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("recent.txt"), text(list));
    }
}

/// A row's label: the file name, then its folder.
fn label(p: &Path) -> String {
    let name = p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned());
    match p.parent().map(|d| d.display().to_string()).filter(|d| !d.is_empty()) {
        Some(d) => format!("{name}   ({d})"),
        None => name,
    }
}

impl App {
    /// Opens the picker at `at`: the recent projects (missing files greyed out), then Open... and Clear list.
    pub(crate) fn project_picker(&mut self, at: (f32, f32)) {
        let list = recents();
        let mut items = vec![PItem::head("Recent projects")];
        if list.is_empty() {
            items.push(PItem::new("No recent projects", Cmd::Nothing).off());
        }
        for (i, p) in list.iter().enumerate() {
            let it = PItem::new(label(p), Cmd::View(VCmd::OpenRecent(i)));
            items.push(if p.is_file() { it } else { it.off() });
        }
        items.extend([PItem::sep(), PItem::new("Open...", Cmd::View(VCmd::OpenDialog)), PItem::new("Clear list", Cmd::View(VCmd::ClearRecent))]);
        self.state.playlist.popup = Some(Popup { x: at.0, y: at.1, items, sub: None });
        self.invalidate();
    }

    /// Opens recent project `i` through the FILE > Open path (unsaved changes are asked about first). Returns a message.
    pub(crate) fn open_recent(&mut self, i: usize) -> Option<String> {
        let path = recents().get(i).cloned()?;
        if !path.is_file() {
            forget(&path);
            return Some(format!("{} is no longer there: removed from the recent projects", path.display()));
        }
        if !self.confirm_discard("opening another project") {
            return None;
        }
        self.open(path);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Action;
    use crate::views::menus::VIEW;

    #[test]
    fn list_keeps_the_newest_once() {
        for i in 0..MAX + 3 {
            note(Path::new(&format!("/p/{i}.flp")));
        }
        note(Path::new("/p/5.flp"));
        let l = recents();
        assert_eq!(l.len(), MAX);
        assert_eq!(l[0], PathBuf::from("/p/5.flp"));
        assert_eq!(l[1], PathBuf::from(format!("/p/{}.flp", MAX + 2)));
        assert_eq!(l.iter().filter(|p| p.ends_with("5.flp")).count(), 1);
        forget(Path::new("/p/5.flp"));
        assert_eq!(recents().len(), MAX - 1);
        clear();
        assert!(recents().is_empty());
    }

    #[test]
    fn file_round_trips() {
        let l = vec![PathBuf::from("/a/b.flp"), PathBuf::from("C:\\x y\\c.flp")];
        assert_eq!(parse(&text(&l)), l);
        assert_eq!(parse("junk\nproject=\nproject=/a.flp\nproject=/a.flp\nother=/b.flp\n"), vec![PathBuf::from("/a.flp")]);
    }

    /// A project opened (or saved under a new name) joins the list; VIEW > Project picker, Ctrl+F8 and the toolbar
    /// button show it, and choosing it opens it through the FILE > Open path.
    #[test]
    fn picker_lists_and_opens_recent_projects() {
        let dir = std::env::temp_dir().join(format!("lemon-picker-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = App::new(None);
        // Save the empty project to a file: it becomes the newest recent project.
        let path = dir.join("recent.flp");
        app.save_to(path.clone());
        crate::log::watch(&mut app);
        assert_eq!(recents().first(), Some(&path));
        let item = (0..60).find(|&i| crate::views::menus::item_at(VIEW, i).is_some_and(|it| it.label == "Project picker")).unwrap();
        assert!(crate::views::menus::works(VIEW, crate::views::menus::item_at(VIEW, item).unwrap()));
        // Ctrl+F8 opens the picker.
        app.key(&winit::keyboard::Key::Named(winit::keyboard::NamedKey::F8), true, false);
        let pop = app.state.playlist.popup.as_ref().expect("Ctrl+F8 opens the project picker");
        let i = pop.items.iter().position(|it| it.cmd == Cmd::View(VCmd::OpenRecent(0))).expect("the saved project is listed");
        assert!(pop.items[i].enabled && pop.items[i].label.contains("recent.flp"));
        // A new project, then the recent one again.
        app.menu_item(crate::views::menus::FILE_MENU, crate::views::menus::FILE_NEW);
        assert!(app.state.path.is_none());
        app.state.playlist.popup = None;
        app.menu_item(VIEW, item);
        let i = app.state.playlist.popup.as_ref().unwrap().items.iter().position(|it| it.cmd == Cmd::View(VCmd::OpenRecent(0))).unwrap();
        app.act(Action::PlPopup(i));
        assert_eq!(app.state.path.as_ref(), Some(&path));
        assert!(app.state.message.as_deref().is_some_and(|m| m.starts_with("Opened")));
        // A file that went away is dropped from the list when chosen.
        std::fs::remove_file(&path).unwrap();
        assert!(crate::playlist::run(&mut app, Cmd::View(VCmd::OpenRecent(0))).is_some_and(|m| m.contains("no longer there")));
        assert!(recents().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The toolbar's project button is no longer a placeholder.
    #[test]
    fn toolbar_button_opens_the_picker() {
        let Ok(mut c) = os_ui::Canvas::new(1920, 1080, 1.0) else { return };
        let mut app = App::new(None);
        app.paint(&mut c, None);
        assert!(!app.hits.iter().any(|h| h.action == Action::Placeholder("Project picker")));
        let h = app.hits.iter().find(|h| h.hint == "Project picker (Ctrl+F8)").expect("the project button").rect;
        app.mouse_move(h.cx(), h.cy());
        app.mouse_down();
        app.mouse_up();
        assert!(app.state.playlist.popup.as_ref().is_some_and(|p| p.items[0].label == "Recent projects"));
    }
}
