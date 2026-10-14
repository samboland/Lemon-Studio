//! The in-app log shown on OPTIONS > Debugging log: the last few hundred messages the hint bar showed (opened, saved,
//! failed, ...), newest last, each with the time it came and whether it reads as an error. Fed by `watch` from the UI
//! tick (every new `State::message`) and by `push` from anywhere else. Kept for the run only, not saved (Lemon Studio
//! choice: FL's debugging log is its own window; Lemon's lives on the Debug page of the settings window).
// spec: ui/settings.md "Debug"

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use crate::app::App;

/// Entries kept; the oldest goes first.
pub const CAP: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Local time, HH:MM:SS.
    pub at: String,
    pub text: String,
    /// The message reads as a failure ("failed", "could not", "error", ...).
    pub error: bool,
}

/// The log and the message it saw last (the `String`'s address and text: a message set again, even to the same
/// words, is a new allocation and logs again).
#[derive(Default)]
struct Log {
    entries: VecDeque<Entry>,
    seen: Option<(usize, String)>,
}

#[cfg_attr(test, allow(dead_code))]
static LOG: OnceLock<Mutex<Log>> = OnceLock::new();

#[cfg(test)]
thread_local! {
    /// Tests run in parallel threads: each has a log of its own.
    static TEST: std::cell::RefCell<Log> = std::cell::RefCell::default();
}

fn with<R>(f: impl FnOnce(&mut Log) -> R) -> R {
    #[cfg(test)]
    return TEST.with_borrow_mut(f);
    #[cfg(not(test))]
    f(&mut LOG.get_or_init(Mutex::default).lock().unwrap_or_else(|e| e.into_inner()))
}

/// Whether `text` reports a failure.
pub fn is_error(text: &str) -> bool {
    let t = text.to_lowercase();
    ["fail", "could not", "cannot", "error", "not found", "crash"].iter().any(|w| t.contains(w))
}

/// Adds a line to the log.
pub fn push(text: impl Into<String>) {
    let text: String = text.into();
    let at = chrono::Local::now().format("%H:%M:%S").to_string();
    let error = is_error(&text);
    with(|l| {
        if l.entries.len() >= CAP {
            l.entries.pop_front();
        }
        l.entries.push_back(Entry { at, text, error });
    });
}

/// The log, oldest first.
pub fn entries() -> Vec<Entry> {
    with(|l| l.entries.iter().cloned().collect())
}

pub fn clear() {
    with(|l| l.entries.clear());
}

/// The UI tick's watcher (one call in `App::tick`): a new hint-bar message goes into the log.
pub fn watch(app: &mut App) {
    if let Some(m) = &app.state.message {
        let key = (m.as_ptr() as usize, m.clone());
        let new = with(|l| {
            let new = l.seen.as_ref() != Some(&key);
            l.seen = Some(key);
            new
        });
        if new {
            push(m.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_last_entries_and_flags_errors() {
        for i in 0..CAP + 5 {
            push(format!("line {i}"));
        }
        let e = entries();
        assert_eq!(e.len(), CAP);
        assert_eq!(e[0].text, "line 5");
        assert_eq!(e[CAP - 1].text, format!("line {}", CAP + 4));
        assert_eq!(e[0].at.len(), 8);
        clear();
        push("Save failed: disk full");
        push("Saved song.flp");
        let e = entries();
        assert_eq!((e[0].error, e[1].error), (true, false));
        assert!(is_error("Could not open x.flp: no such file"));
    }

    /// Every new hint-bar message goes into the log once, also when the same words come again.
    #[test]
    fn watch_logs_each_message_once() {
        let mut app = App::new(None);
        clear();
        app.state.message = Some("Opened a.flp".into());
        watch(&mut app);
        watch(&mut app);
        assert_eq!(entries().iter().map(|e| e.text.as_str()).collect::<Vec<_>>(), ["Opened a.flp"]);
        let keep = app.state.message.take();
        app.state.message = Some("Metronome on".into());
        watch(&mut app);
        drop(keep);
        assert_eq!(entries().len(), 2);
        app.state.message = None;
        watch(&mut app);
        assert_eq!(entries().len(), 2);
    }
}
