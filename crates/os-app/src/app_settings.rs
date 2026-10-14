//! The program settings of Settings > General that work, and the File page's autosave interval: kept in the user
//! folder (`general.txt`), not in the project. The rows are `views/settings/general.rs` and `file.rs`; what each
//! setting changes is read here by its consumer (undo depth, note names, where an automation clip starts), and the
//! ones that follow the editors (linked mixer track, first channel with notes, auto zip) run from `tick`, on the app's
//! own tick.
// spec: ui/settings.md "General", "File"

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::app::{App, State};

/// Maximum undo levels: the steps Fewer / More go through (Lemon Studio choice; FL's default is 100).
pub const UNDO_STEPS: [usize; 7] = [10, 25, 50, 100, 200, 500, 1000];

/// Note names (spec "Language"): English (C D E) or solfège (Do Re Mi), the Lemon Studio choice of FL's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Names {
    #[default]
    English,
    Solfege,
}

/// Autosave intervals in minutes (0 = off), with their File page labels (Lemon Studio choice of steps).
pub const AUTOSAVE: [(u32, &str); 5] = [
    (0, "Autosave off"),
    (1, "Very frequently (every minute)"),
    (5, "Frequently (every 5 minutes)"),
    (10, "Occasionally (every 10 minutes)"),
    (30, "Rarely (every 30 minutes)"),
];

/// The settings, with FL's defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct General {
    /// Maximum undo levels: snapshots each editor keeps (piano roll, playlist).
    pub undo_levels: usize,
    /// Auto zip empty channels: when the pattern changes, channels without notes in it are zipped, the others unzipped.
    pub auto_zip: bool,
    /// Auto select linked modules: selecting a channel selects the mixer track it is routed to.
    pub linked: bool,
    /// Create automation clips at the playback position (off: at the start of the song).
    pub automation_at_pos: bool,
    /// Select first channel with notes when the pattern changes.
    pub first_with_notes: bool,
    pub names: Names,
    /// Black notes: flats (Db) instead of sharps (C#).
    pub flats: bool,
    /// File > Autosave: minutes between backups of a changed project (0 = off).
    pub autosave: u32,
}

impl Default for General {
    fn default() -> Self {
        General { undo_levels: 100, auto_zip: false, linked: true, automation_at_pos: true, first_with_notes: false, names: Names::English, flats: false, autosave: 10 }
    }
}

#[cfg_attr(test, allow(dead_code))]
static SETTINGS: OnceLock<Mutex<General>> = OnceLock::new();

#[cfg(test)]
thread_local! {
    /// Tests run in parallel threads: each has its own settings (read again after `forget`) and its own user folder
    /// (none unless `test_dir` sets one), so one test never sees another's.
    static TEST: std::cell::Cell<Option<General>> = const { std::cell::Cell::new(None) };
    static TEST_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// The user folder the settings and backups go to (tests: the folder `test_dir` set on this thread, else none).
pub fn user_dir() -> Option<PathBuf> {
    #[cfg(test)]
    return TEST_DIR.with_borrow(Clone::clone);
    #[cfg(not(test))]
    os_paths::user_dir()
}

/// Tests: this thread's user folder.
#[cfg(test)]
pub fn test_dir(dir: Option<PathBuf>) {
    TEST_DIR.with_borrow_mut(|d| *d = dir);
}

/// Tests: drop this thread's settings, as quitting does; the next `get` reads `general.txt` again.
#[cfg(test)]
pub fn forget() {
    TEST.with(|c| c.set(None));
}

/// Runs `f` on the settings (read from `general.txt` the first time); keeps them when `f` changed them.
fn with<R>(f: impl FnOnce(&mut General) -> R) -> R {
    #[cfg(test)]
    return TEST.with(|c| {
        let mut s = c.get().unwrap_or_else(load);
        let before = s;
        let r = f(&mut s);
        if s != before {
            save(&s);
        }
        c.set(Some(s));
        r
    });
    #[cfg(not(test))]
    {
        let mut s = SETTINGS.get_or_init(|| Mutex::new(load())).lock().unwrap_or_else(|e| e.into_inner());
        let before = *s;
        let r = f(&mut s);
        if *s != before {
            save(&s);
        }
        r
    }
}

pub fn get() -> General {
    with(|s| *s)
}

/// Changes the settings and keeps them.
pub fn set(f: impl FnOnce(&mut General)) {
    with(f)
}

pub fn undo_levels() -> usize {
    get().undo_levels
}

/// The next (`up`) or previous step of `UNDO_STEPS` from `n`, kept within the steps.
pub fn undo_step(n: usize, up: bool) -> usize {
    if up {
        UNDO_STEPS.into_iter().find(|&k| k > n).unwrap_or(UNDO_STEPS[UNDO_STEPS.len() - 1])
    } else {
        UNDO_STEPS.into_iter().rev().find(|&k| k < n).unwrap_or(UNDO_STEPS[0])
    }
}

/// Where a new automation clip starts without a time selection: the song position, or the song's start when
/// "Create automation clips at the playback position" is off.
pub fn automation_at(pos: u32) -> u32 {
    if get().automation_at_pos {
        pos
    } else {
        0
    }
}

/// A note's name in the chosen Note names and Black notes ("C#5", "Db5", "Do#5"): FL's octaves, C5 = MIDI 60.
pub fn note_name(key: i32) -> String {
    const EN: [[&str; 12]; 2] = [
        ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"],
        ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"],
    ];
    const DO: [[&str; 12]; 2] = [
        ["Do", "Do#", "Re", "Re#", "Mi", "Fa", "Fa#", "Sol", "Sol#", "La", "La#", "Si"],
        ["Do", "Reb", "Re", "Mib", "Mi", "Fa", "Solb", "Sol", "Lab", "La", "Sib", "Si"],
    ];
    let g = get();
    let names = match g.names {
        Names::English => &EN,
        Names::Solfege => &DO,
    };
    format!("{}{}", names[usize::from(g.flats)][key.rem_euclid(12) as usize], key.div_euclid(12))
}

/// `key=value` lines (see `text`); anything else is ignored, a missing or bad value keeps its default.
pub fn parse(text: &str) -> General {
    let mut s = General::default();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        let on = v == "1";
        match k {
            "undo_levels" => s.undo_levels = v.parse().ok().filter(|n| UNDO_STEPS.contains(n)).unwrap_or(s.undo_levels),
            "auto_zip" => s.auto_zip = on,
            "linked" => s.linked = on,
            "automation_at_pos" => s.automation_at_pos = on,
            "first_with_notes" => s.first_with_notes = on,
            "note_names" => s.names = if v == "solfege" { Names::Solfege } else { Names::English },
            "flats" => s.flats = on,
            "autosave" => s.autosave = v.parse().ok().filter(|m| AUTOSAVE.iter().any(|a| a.0 == *m)).unwrap_or(s.autosave),
            _ => {}
        }
    }
    s
}

pub fn text(s: &General) -> String {
    let b = u8::from;
    format!(
        "undo_levels={}\nauto_zip={}\nlinked={}\nautomation_at_pos={}\nfirst_with_notes={}\nnote_names={}\nflats={}\nautosave={}\n",
        s.undo_levels,
        b(s.auto_zip),
        b(s.linked),
        b(s.automation_at_pos),
        b(s.first_with_notes),
        if s.names == Names::Solfege { "solfege" } else { "english" },
        b(s.flats),
        s.autosave
    )
}

fn load() -> General {
    user_dir().and_then(|d| std::fs::read_to_string(d.join("general.txt")).ok()).map(|t| parse(&t)).unwrap_or_default()
}

fn save(s: &General) {
    if let Some(dir) = user_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("general.txt"), text(s));
    }
}

/// What the editors showed at the last tick: which app, the pattern, the selected channel.
#[derive(Clone, Copy, PartialEq)]
struct Seen {
    app: usize,
    pattern: u16,
    channel: usize,
}

thread_local! {
    static SEEN: std::cell::Cell<Option<Seen>> = const { std::cell::Cell::new(None) };
}

/// On the app's tick: the settings that follow the editors, then autosave. A pattern change zips the empty channels
/// and selects the first one with notes (each when on); a new selected channel selects its mixer track (Auto select
/// linked modules). The first tick of an app only looks.
pub fn tick(app: &mut App) {
    let g = get();
    let id = app as *const App as usize;
    let s = &mut app.state;
    let now = Seen { app: id, pattern: s.pattern, channel: s.selected_channel };
    let before = SEEN.with(|c| c.replace(Some(now)));
    if let Some(b) = before.filter(|b| b.app == now.app) {
        let mut changed = false;
        if b.pattern != now.pattern {
            if g.auto_zip {
                changed |= zip_empty(s);
            }
            if g.first_with_notes {
                if let Some(row) = first_with_notes(s).filter(|&r| r != s.selected_channel) {
                    s.selected_channel = row;
                    changed = true;
                }
            }
        }
        if g.linked && s.selected_channel != b.channel {
            if let Some(t) = s.project.channels.get(s.selected_channel).map(|c| c.mixer_track).filter(|&t| t != s.selected_mixer) {
                s.selected_mixer = t;
                changed = true;
            }
        }
        SEEN.with(|c| c.set(Some(Seen { channel: s.selected_channel, ..now })));
        if changed {
            app.invalidate();
        }
    }
    crate::autosave::tick(app, std::time::Instant::now());
}

/// Channel indexes with notes or steps in the current pattern.
fn used(s: &State) -> std::collections::BTreeSet<u16> {
    s.project.patterns.iter().find(|p| p.number == s.pattern).map(|p| p.notes.iter().map(|n| n.channel).collect()).unwrap_or_default()
}

/// The rack row of the first channel with notes in the current pattern.
pub fn first_with_notes(s: &State) -> Option<usize> {
    let used = used(s);
    s.project.channels.iter().position(|c| used.contains(&c.index))
}

/// Zips the channels without notes in the current pattern and unzips the others; returns whether any changed.
pub fn zip_empty(s: &mut State) -> bool {
    let used = used(s);
    let zipped: std::collections::BTreeSet<usize> = s.project.channels.iter().enumerate().filter(|(_, c)| !used.contains(&c.index)).map(|(r, _)| r).collect();
    if s.rack.zipped == zipped {
        return false;
    }
    s.rack.zipped = zipped;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh user folder for this test.
    pub(crate) fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lemon-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn settings_file_round_trips_with_defaults() {
        let d = parse("");
        assert_eq!(d, General::default());
        assert_eq!((d.undo_levels, d.linked, d.automation_at_pos, d.autosave), (100, true, true, 10));
        let s = General { undo_levels: 500, auto_zip: true, linked: false, automation_at_pos: false, first_with_notes: true, names: Names::Solfege, flats: true, autosave: 5 };
        assert_eq!(parse(&text(&s)), s);
        // Values outside the steps keep the default.
        let bad = parse("undo_levels=7\nautosave=3\njunk\nflats=1");
        assert_eq!((bad.undo_levels, bad.autosave, bad.flats), (100, 10, true));
    }

    #[test]
    fn undo_steps_stay_in_range() {
        assert_eq!(undo_step(100, true), 200);
        assert_eq!(undo_step(100, false), 50);
        assert_eq!(undo_step(1000, true), 1000);
        assert_eq!(undo_step(10, false), 10);
        // A value between steps goes to the neighbouring step.
        assert_eq!((undo_step(150, true), undo_step(150, false)), (200, 100));
    }

    #[test]
    fn note_names_follow_the_language_settings() {
        test_dir(None);
        forget();
        assert_eq!((note_name(60), note_name(61), note_name(70)), ("C5".into(), "C#5".into(), "A#5".into()));
        set(|s| s.flats = true);
        assert_eq!((note_name(61), note_name(70), note_name(-1)), ("Db5".into(), "Bb5".into(), "B-1".into()));
        set(|s| s.names = Names::Solfege);
        assert_eq!((note_name(61), note_name(67)), ("Reb5".into(), "Sol5".into()));
        set(|s| s.flats = false);
        assert_eq!(note_name(66), "Fa#5");
        forget();
    }

    /// The settings persist: set, quit (`forget`), read again from `general.txt`.
    #[test]
    fn settings_persist_in_the_user_folder() {
        let dir = temp_dir("general");
        test_dir(Some(dir.clone()));
        forget();
        assert_eq!(get(), General::default());
        set(|s| {
            s.undo_levels = 25;
            s.auto_zip = true;
        });
        assert!(std::fs::read_to_string(dir.join("general.txt")).unwrap().contains("undo_levels=25\nauto_zip=1\n"));
        forget();
        let g = get();
        assert_eq!((g.undo_levels, g.auto_zip), (25, true));
        test_dir(None);
        forget();
        let _ = std::fs::remove_dir_all(dir);
    }
}
