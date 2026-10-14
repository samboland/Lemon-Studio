//! Autosave and FILE > Revert to last backup. Every N minutes (Settings > File > Autosave) a project with unsaved
//! changes is written as a backup to the `Backup` folder of the user folder, as "name (autosave 2026-10-14
//! 153012).flp"; the project itself, its file and its saved state are left alone. The newest 100 backups of each project
//! name are kept. Revert opens the current project's newest backup.
// spec: ui/settings.md "File", ui/menus.md (FILE)

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use os_model::Project;

use crate::app::App;

/// Backups kept per project name (the File page's Maximum, fixed for now).
pub const MAX_BACKUPS: usize = 100;

/// The timer: when the current interval started, and what the last backup held.
#[derive(Default)]
struct Timer {
    since: Option<Instant>,
    last: Option<(Project, f64)>,
}

thread_local! {
    static TIMER: std::cell::RefCell<Timer> = std::cell::RefCell::default();
}

/// `Backup` in the user folder (tests: this thread's test folder).
pub fn backup_dir() -> Option<PathBuf> {
    crate::app_settings::user_dir().map(|d| d.join("Backup"))
}

/// The project's name for its backups: its file's name, else "Untitled".
fn stem(app: &App) -> String {
    app.state.path.as_ref().and_then(|p| p.file_stem()).map_or_else(|| "Untitled".to_string(), |n| n.to_string_lossy().into_owned())
}

/// On the app's tick at `now`: with autosave on, every interval a project with unsaved changes is backed up, unless
/// the last backup holds it as it is now. The project is only compared once an interval has passed.
pub fn tick(app: &mut App, now: Instant) {
    let mins = crate::app_settings::get().autosave;
    if mins == 0 {
        TIMER.with_borrow_mut(|t| t.since = None);
        return;
    }
    let since = TIMER.with_borrow_mut(|t| *t.since.get_or_insert(now));
    if now.duration_since(since) < Duration::from_secs(u64::from(mins) * 60) {
        return;
    }
    TIMER.with_borrow_mut(|t| t.since = Some(now));
    if !app.unsaved() {
        return;
    }
    let same = TIMER.with_borrow(|t| t.last.as_ref().is_some_and(|(p, tempo)| *p == app.state.project && *tempo == app.state.tempo));
    if same {
        return;
    }
    match backup(app) {
        Ok(Some(p)) => {
            TIMER.with_borrow_mut(|t| t.last = Some((app.state.project.clone(), app.state.tempo)));
            app.state.message = Some(format!("Autosaved a backup: {}", p.display()));
            app.invalidate();
        }
        Ok(None) => {}
        Err(e) => {
            app.state.message = Some(format!("Autosave failed: {e}"));
            app.invalidate();
        }
    }
}

/// Writes the project as it is now to a new backup file; returns its path (`None`: no user folder).
pub fn backup(app: &mut App) -> Result<Option<PathBuf>, String> {
    let Some(dir) = backup_dir() else { return Ok(None) };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stem = stem(app);
    let time = chrono::Local::now().format("%Y-%m-%d %H%M%S").to_string();
    let target = (1..)
        .map(|k| dir.join(if k == 1 { format!("{stem} (autosave {time}).flp") } else { format!("{stem} (autosave {time}-{k}).flp") }))
        .find(|p| !p.exists())
        .expect("a free name");
    write_copy(app, &target)?;
    prune(&dir, &stem);
    Ok(Some(target))
}

/// Writes the project to `target` as Save would (`App::save_to`) without making `target` the project's file: the
/// loaded file is edited in a copy, the project's own channel order is kept.
fn write_copy(app: &mut App, target: &Path) -> Result<(), String> {
    app.store_builtin_states();
    let s = &mut app.state;
    crate::rack::to_model(s);
    crate::playlist::store_view(s);
    let mut proj = s.project.clone();
    proj.tempo = Some(s.tempo);
    // Sampler channels keep their sample in Lemon Sampler: the backup stores it on the channel, as a save does.
    for &row in &s.hosted_rows {
        if proj.channels.get(row).is_some_and(|c| c.kind.is_sampler()) {
            if let Some(p) = app.plugins.sampler_path(row) {
                proj.channels[row].sample_path = (!p.is_empty()).then_some(p);
            }
        }
    }
    let lacks_playlist = s.file.as_ref().and_then(|f| os_flp::project::load(f).ok()).is_some_and(|p| p.arrangements.is_empty()) && !proj.arrangements.is_empty();
    let mut file = match &s.file {
        Some(f) if !lacks_playlist => f.clone(),
        _ => {
            for (i, c) in proj.channels.iter_mut().enumerate() {
                c.rack.origin = Some(i as u16);
            }
            if proj.arrangements.is_empty() {
                proj.arrangements.push(os_flp::project::default_arrangement());
            }
            let mut first = proj.clone();
            crate::project_plugins::store_hosted(&mut first, &app.plugins);
            os_flp::project::create(&first).map_err(|e| e.to_string())?
        }
    };
    crate::project_plugins::store_hosted(&mut proj, &app.plugins);
    os_flp::project::apply(&proj, &mut file).map_err(|e| e.to_string())?;
    let bytes = file.to_bytes().map_err(|e| e.to_string())?;
    std::fs::write(target, bytes).map_err(|e| e.to_string())
}

/// The backups of project `stem` in `dir`, oldest first (their names sort by time).
pub fn backups(dir: &Path, stem: &str) -> Vec<PathBuf> {
    let head = format!("{stem} (autosave ");
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&head) && n.ends_with(").flp")))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Removes the oldest backups of `stem` beyond `MAX_BACKUPS`.
fn prune(dir: &Path, stem: &str) {
    let v = backups(dir, stem);
    for p in v.iter().take(v.len().saturating_sub(MAX_BACKUPS)) {
        let _ = std::fs::remove_file(p);
    }
}

/// The current project's newest backup.
pub fn newest(app: &App) -> Option<PathBuf> {
    backups(&backup_dir()?, &stem(app)).pop()
}

/// FILE > Revert to last backup: asks, then opens the project's newest backup in place of the project. The project
/// keeps its own file name (Save writes there, not over the backup).
pub fn revert(app: &mut App) {
    let Some(p) = newest(app) else {
        app.state.message = Some(format!("No backup of {} yet (Settings > File > Autosave)", stem(app)));
        return app.invalidate();
    };
    let name = p.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let ok = cfg!(test)
        || match crate::dialog::yes_no_cancel("Revert to last backup", &format!("Open the backup {name}? Changes made since then are lost.")) {
            Some(crate::dialog::Answer::Yes) => true,
            Some(_) => false,
            // No dialog program: nothing unsaved is lost unasked.
            None => app.confirm_discard("reverting to the last backup"),
        };
    if !ok {
        return;
    }
    let path = app.state.path.clone();
    app.open(p);
    if app.state.message.as_deref().is_some_and(|m| m.starts_with("Opened")) {
        app.state.path = path;
        app.state.message = Some(format!("Reverted to the backup {name}"));
    }
    app.invalidate();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_settings as cfg;

    /// Autosave on a test clock: nothing before the interval, a backup after it, none again while nothing changed, none
    /// when off; Revert opens the newest backup.
    #[test]
    fn autosave_writes_backups_and_revert_opens_the_newest() {
        let dir = std::env::temp_dir().join(format!("lemon-autosave-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        cfg::test_dir(Some(dir.clone()));
        cfg::forget();
        cfg::set(|s| s.autosave = 5);
        let mut app = App::new(None);
        let t0 = Instant::now();
        let bdir = dir.join("Backup");
        tick(&mut app, t0);
        tick(&mut app, t0 + Duration::from_secs(300));
        assert!(backups(&bdir, "Untitled").is_empty(), "nothing changed");
        app.state.tempo = 140.0;
        assert!(app.unsaved());
        tick(&mut app, t0 + Duration::from_secs(599));
        assert!(backups(&bdir, "Untitled").is_empty(), "not yet: 5 minutes");
        tick(&mut app, t0 + Duration::from_secs(600));
        let first = backups(&bdir, "Untitled");
        assert_eq!(first.len(), 1, "a backup after 5 minutes");
        assert!(app.state.message.as_deref().unwrap().starts_with("Autosaved a backup"));
        assert!(app.unsaved() && app.state.path.is_none(), "the project stays as it was");
        tick(&mut app, t0 + Duration::from_secs(900));
        assert_eq!(backups(&bdir, "Untitled").len(), 1, "unchanged since the last backup");
        app.state.tempo = 150.0;
        tick(&mut app, t0 + Duration::from_secs(1000));
        assert_eq!(backups(&bdir, "Untitled").len(), 1, "the next interval has not passed");
        tick(&mut app, t0 + Duration::from_secs(1200));
        assert_eq!(backups(&bdir, "Untitled").len(), 2);
        // Off: no more backups.
        cfg::set(|s| s.autosave = 0);
        app.state.tempo = 160.0;
        tick(&mut app, t0 + Duration::from_secs(10_000));
        assert_eq!(backups(&bdir, "Untitled").len(), 2);

        // FILE > Revert to last backup: the newest (tempo 150).
        let menu = crate::views::menus::FILE_MENU;
        assert!(crate::menu_registry::handles(menu, "Revert to last backup"), "the menu entry works");
        crate::menu_file::menu_run(&mut app, menu, "Revert to last backup");
        assert_eq!(app.state.tempo, 150.0);
        assert!(!app.unsaved());
        assert_eq!(app.state.path, None, "an untitled project stays untitled");
        assert!(app.state.message.as_deref().unwrap().starts_with("Reverted to the backup"));

        cfg::set(|s| s.autosave = 10);
        cfg::test_dir(None);
        cfg::forget();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn revert_without_backups_says_so() {
        cfg::test_dir(None);
        let mut app = App::new(None);
        revert(&mut app);
        assert!(app.state.message.as_deref().unwrap().starts_with("No backup of Untitled"));
    }

    #[test]
    fn old_backups_are_pruned() {
        let dir = std::env::temp_dir().join(format!("lemon-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for k in 0..MAX_BACKUPS + 3 {
            std::fs::write(dir.join(format!("song (autosave 2026-01-01 {k:06}).flp")), b"").unwrap();
        }
        std::fs::write(dir.join("other (autosave 2026-01-01 000000).flp"), b"").unwrap();
        prune(&dir, "song");
        let left = backups(&dir, "song");
        assert_eq!(left.len(), MAX_BACKUPS);
        assert!(left[0].ends_with("song (autosave 2026-01-01 000003).flp"), "the oldest went");
        assert_eq!(backups(&dir, "other").len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
