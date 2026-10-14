//! Settings > Info: the project credits (title, author, genre, comments, web link) and its counts. The credits are
//! text fields (`os_ui::kit::text_field`): [`start`] gives one the keyboard and [`key`] edits it. Each keystroke
//! writes the project's info straight away, so a save, or closing the window, keeps what was typed; Esc puts the
//! field back as it was. The file keeps them as the project-info events (spec: flp/events/project-info.md).
// spec: ui/settings.md "Info"

use std::cell::RefCell;

use os_model::ProjectInfo;
use os_ui::kit::text_field::Field;
use winit::keyboard::{Key, NamedKey};

use super::{g, txt, Group, Row, SAct};
use crate::app::{App, State};

/// The checks and buttons of this page that work (`SAct::Check` / `SAct::Button` labels, handled in `act`).
pub const LIVE: &[&str] = &[];

/// One credits field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credit {
    Title,
    Author,
    Genre,
    /// Several lines: Enter starts a new one.
    Comments,
    Url,
}

impl Credit {
    /// In page order (Tab moves to the next one).
    pub const ALL: [Credit; 5] = [Credit::Title, Credit::Author, Credit::Genre, Credit::Comments, Credit::Url];

    pub fn label(self) -> &'static str {
        match self {
            Credit::Title => "Title",
            Credit::Author => "Author",
            Credit::Genre => "Genre",
            Credit::Comments => "Comments",
            Credit::Url => "Web link",
        }
    }

    pub fn multiline(self) -> bool {
        self == Credit::Comments
    }

    pub fn get(self, i: &ProjectInfo) -> &Option<String> {
        match self {
            Credit::Title => &i.title,
            Credit::Author => &i.author,
            Credit::Genre => &i.genre,
            Credit::Comments => &i.comments,
            Credit::Url => &i.url,
        }
    }

    fn get_mut(self, i: &mut ProjectInfo) -> &mut Option<String> {
        match self {
            Credit::Title => &mut i.title,
            Credit::Author => &mut i.author,
            Credit::Genre => &mut i.genre,
            Credit::Comments => &mut i.comments,
            Credit::Url => &mut i.url,
        }
    }
}

/// The field that has the keyboard: what it holds and what it held before (for Esc).
struct Edit {
    credit: Credit,
    field: Field,
    before: Option<String>,
}

thread_local! {
    /// Session state of the page (the UI thread's; each test thread has its own).
    static EDIT: RefCell<Option<Edit>> = const { RefCell::new(None) };
}

/// The field being edited and its caret (byte offset), if any.
pub fn editing() -> Option<(Credit, usize)> {
    EDIT.with(|e| e.borrow().as_ref().map(|e| (e.credit, e.field.caret)))
}

/// Gives field `credit` the keyboard, the caret at the end of its text (a click on the field).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn start(app: &mut App, credit: Credit) {
    let before = credit.get(&app.state.project.info).clone();
    let field = Field::new(before.as_deref().unwrap_or(""), credit.multiline());
    EDIT.with(|e| *e.borrow_mut() = Some(Edit { credit, field, before }));
    app.invalidate();
}

/// The field gives the keyboard back, keeping its text.
pub(crate) fn stop() {
    EDIT.with(|e| *e.borrow_mut() = None);
}

/// A key while a credits field has the keyboard: typing and the editing keys change it; Enter (a new line in
/// Comments) and Tab (to the next field, Lemon Studio choice) end it; Esc puts it back. Ctrl / Alt combinations stay
/// shortcuts (Ctrl+S saves what was typed). Returns whether the key was used. The field lets go when the Info page is
/// no longer shown.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn key(app: &mut App, key: &Key, ctrl: bool, alt: bool) -> bool {
    let Some(credit) = editing().map(|e| e.0) else { return false };
    let st = &app.state.settings;
    if !st.open || st.tab != super::INFO {
        stop();
        return false;
    }
    if ctrl || alt {
        return false;
    }
    let done = EDIT.with(|e| {
        let mut e = e.borrow_mut();
        let ed = e.as_mut().expect("editing");
        let f = &mut ed.field;
        match key {
            Key::Character(c) => f.insert(c),
            Key::Named(NamedKey::Space) => f.insert(" "),
            Key::Named(NamedKey::Backspace) => {
                f.backspace();
            }
            Key::Named(NamedKey::Delete) => {
                f.delete();
            }
            Key::Named(NamedKey::ArrowLeft) => f.left(),
            Key::Named(NamedKey::ArrowRight) => f.right(),
            Key::Named(NamedKey::ArrowUp) => f.up(),
            Key::Named(NamedKey::ArrowDown) => f.down(),
            Key::Named(NamedKey::Home) => f.home(),
            Key::Named(NamedKey::End) => f.end(),
            Key::Named(NamedKey::Enter) if f.newline() => {}
            Key::Named(NamedKey::Enter) => return Some(None),
            Key::Named(NamedKey::Tab) => {
                let i = Credit::ALL.iter().position(|&c| c == credit).unwrap_or(0);
                return Some(Some(Credit::ALL[(i + 1) % Credit::ALL.len()]));
            }
            Key::Named(NamedKey::Escape) => {
                f.text = ed.before.clone().unwrap_or_default();
                return Some(None);
            }
            // Other keys do nothing while typing (no shortcuts, no notes).
            _ => {}
        }
        None
    });
    // What the field holds is the project's, at once (empty: no text, as a project without it).
    let text = EDIT.with(|e| e.borrow().as_ref().map(|e| e.field.text.clone())).unwrap_or_default();
    let before = EDIT.with(|e| e.borrow().as_ref().and_then(|e| e.before.clone()));
    let value = if text.is_empty() && before.is_none() { None } else { Some(text) };
    let slot = credit.get_mut(&mut app.state.project.info);
    if *slot != value {
        *slot = value;
    }
    match done {
        Some(Some(next)) => start(app, next),
        Some(None) => stop(),
        None => {}
    }
    app.invalidate();
    true
}

/// The Info page's groups (spec §4).
pub fn page(s: &State) -> Vec<Group> {
    use Row::*;
    let i = &s.project.info;
    let or = |v: &Option<String>| v.clone().unwrap_or_default();
    let clips: usize = s.project.arrangements.iter().map(|a| a.clips.len()).sum();
    let notes: usize = s.project.patterns.iter().map(|p| p.notes.len()).sum();
    let mut rows = Vec::new();
    for c in Credit::ALL {
        rows.push(Text(c.label().into(), true));
        let v = or(c.get(i));
        if c.multiline() {
            rows.push(List(vec![("", 0.0)], v.lines().map(|l| vec![l.to_string()]).collect(), 14));
        } else {
            rows.push(Big(v));
        }
    }
    rows.push(Gap(8.0));
    rows.push(txt(format!("Channels: {}, Mixer Tracks: {}", s.project.channels.len(), s.project.mixer.len())));
    rows.push(txt(format!("Arrangements: {}, PL clips: {clips}, Notes: {notes}", s.project.arrangements.len())));
    vec![g("Project credits", rows)]
}

/// This page's working controls (`LIVE`); none yet.
pub(crate) fn act(_app: &mut App, _a: &SAct) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(app: &mut App, k: Key) -> bool {
        key(app, &k, false, false)
    }

    fn typed(app: &mut App, s: &str) {
        for ch in s.chars() {
            let k = if ch == ' ' { Key::Named(NamedKey::Space) } else { Key::Character(ch.to_string().into()) };
            assert!(press(app, k), "{ch:?} typed");
        }
    }

    /// Every credits field is typed into (Tab moves on, Enter makes lines in Comments), the project's info follows
    /// each key, and the saved file opens again with every value.
    #[test]
    fn credits_are_typed_saved_and_reopened() {
        let mut app = App::new(None);
        app.open_settings(super::super::INFO);
        start(&mut app, Credit::Title);
        typed(&mut app, "Lemon tree");
        assert_eq!(app.state.project.info.title.as_deref(), Some("Lemon tree"), "written at once");
        press(&mut app, Key::Named(NamedKey::Backspace));
        press(&mut app, Key::Named(NamedKey::ArrowLeft));
        press(&mut app, Key::Named(NamedKey::Home));
        typed(&mut app, "A ");
        assert_eq!(app.state.project.info.title.as_deref(), Some("A Lemon tre"));
        press(&mut app, Key::Named(NamedKey::Tab));
        assert_eq!(editing().map(|e| e.0), Some(Credit::Author));
        typed(&mut app, "Sam");
        press(&mut app, Key::Named(NamedKey::Tab));
        typed(&mut app, "Downtempo");
        press(&mut app, Key::Named(NamedKey::Tab));
        typed(&mut app, "first");
        press(&mut app, Key::Named(NamedKey::Enter));
        typed(&mut app, "second");
        assert_eq!(editing().map(|e| e.0), Some(Credit::Comments), "Enter makes a line in Comments");
        press(&mut app, Key::Named(NamedKey::Tab));
        typed(&mut app, "https://lemon.example");
        press(&mut app, Key::Named(NamedKey::Enter));
        assert_eq!(editing(), None, "Enter ends a one-line field");
        assert!(!press(&mut app, Key::Character("x".into())), "no field has the keyboard");
        let want = ProjectInfo {
            title: Some("A Lemon tre".into()),
            author: Some("Sam".into()),
            genre: Some("Downtempo".into()),
            comments: Some("first\nsecond".into()),
            url: Some("https://lemon.example".into()),
        };
        assert_eq!(app.state.project.info, want);
        assert!(app.unsaved());
        let path = std::env::temp_dir().join(format!("lemon-credits-{}.flp", std::process::id()));
        app.save_to(path.clone());
        assert!(app.state.message.as_deref().is_some_and(|m| m.starts_with("Saved")), "{:?}", app.state.message);
        let mut back = App::new(None);
        back.open(path.clone());
        assert_eq!(back.state.project.info, want);
        // Changed again in the reopened project: the file's own events take the new text.
        back.open_settings(super::super::INFO);
        start(&mut back, Credit::Genre);
        for _ in 0.."Downtempo".len() {
            press(&mut back, Key::Named(NamedKey::Backspace));
        }
        typed(&mut back, "Dub");
        back.save_to(path.clone());
        let mut again = App::new(None);
        again.open(path.clone());
        let _ = std::fs::remove_file(&path);
        assert_eq!(again.state.project.info, ProjectInfo { genre: Some("Dub".into()), ..want });
    }

    /// Esc puts the field back; shortcuts pass; the field lets go when the page is not shown.
    #[test]
    fn escape_restores_and_the_field_lets_go() {
        let mut app = App::new(None);
        app.state.project.info.title = Some("Kept".into());
        app.mark_saved();
        app.open_settings(super::super::INFO);
        start(&mut app, Credit::Title);
        typed(&mut app, " and more");
        assert_eq!(app.state.project.info.title.as_deref(), Some("Kept and more"));
        press(&mut app, Key::Named(NamedKey::Escape));
        assert_eq!(app.state.project.info.title.as_deref(), Some("Kept"));
        assert_eq!(editing(), None);
        // A field that was empty stays without text.
        start(&mut app, Credit::Url);
        typed(&mut app, "x");
        press(&mut app, Key::Named(NamedKey::Backspace));
        assert_eq!(app.state.project.info.url, None);
        assert!(!key(&mut app, &Key::Character("s".into()), true, false), "Ctrl+S stays a shortcut");
        assert!(editing().is_some());
        app.state.settings.open = false;
        assert!(!press(&mut app, Key::Character("y".into())), "the window closed: the field let go");
        assert_eq!(editing(), None);
        assert!(!app.unsaved(), "nothing changed in the end");
    }
}
