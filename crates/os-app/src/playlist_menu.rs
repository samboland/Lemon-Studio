//! Main-menu entries of the playlist (`menu_registry` area): TOOLS > Lock all playlist tracks to content / Unlock all
//! playlist tracks from content, and ADD > Automation clip (an automation clip linked to nothing).
// spec: ui/menus.md (ADD, TOOLS), ui/playlist.md §6, §8

use crate::app::{App, Win};
use crate::views::menus::{ADD_MENU, TOOLS_MENU};

pub const LOCK_ALL: &str = "Lock all playlist tracks to content";
pub const UNLOCK_ALL: &str = "Unlock all playlist tracks from content";
pub const AUTOMATION_CLIP: &str = "Automation clip";

pub fn menu_handles(menu: usize, label: &str) -> bool {
    (menu == TOOLS_MENU && matches!(label, LOCK_ALL | UNLOCK_ALL)) || (menu == ADD_MENU && label == AUTOMATION_CLIP)
}

pub fn menu_run(app: &mut App, _menu: usize, label: &str) -> Option<String> {
    let s = &mut app.state;
    let msg = match label {
        LOCK_ALL | UNLOCK_ALL => {
            let on = label == LOCK_ALL;
            let n = crate::playlist::lock_all(s, on);
            let tracks = if n == 1 { "track" } else { "tracks" };
            match (n, on) {
                (0, true) => "No playlist track with clips to lock".to_string(),
                (0, false) => "No playlist track is locked".to_string(),
                (n, true) => format!("Locked {n} {tracks} to their content"),
                (n, false) => format!("Unlocked {n} {tracks}"),
            }
        }
        AUTOMATION_CLIP => {
            let ch = crate::playlist::create_unlinked_automation(s);
            if !s.shown(Win::Playlist) || !s.in_front(Win::Playlist) {
                s.raise(Win::Playlist);
            }
            format!("Added {} (not linked to a control)", crate::playlist::source_name(s, os_model::ClipSource::Channel(ch)))
        }
        _ => return None,
    };
    app.invalidate();
    Some(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playlist as pl;
    use crate::views::menus::{item_at, works, Kind};
    use os_model::{Clip, ClipSource};

    /// The flat index of `label` in `menu` (tests drive `App::menu_item` as a click does).
    fn index(menu: usize, label: &str) -> usize {
        (0..200).find(|&i| item_at(menu, i).is_some_and(|it| it.label == label && it.kind == Kind::Item)).expect(label)
    }

    fn app() -> App {
        let mut app = App::new(None);
        app.state.project = crate::app::test_project();
        app.state.playlist.reset();
        pl::arr_mut(&mut app.state).clips = vec![Clip::new(0, 384, 0, ClipSource::Pattern(1)), Clip::new(0, 384, 3, ClipSource::Pattern(1))];
        app
    }

    /// The entries work in this build (no longer "not available yet").
    #[test]
    fn entries_work() {
        for (menu, label) in [(TOOLS_MENU, LOCK_ALL), (TOOLS_MENU, UNLOCK_ALL), (ADD_MENU, AUTOMATION_CLIP)] {
            let item = item_at(menu, index(menu, label)).unwrap();
            assert!(works(menu, item), "{label}");
            assert!(crate::menu_registry::handles(menu, label));
        }
    }

    /// TOOLS > Lock all locks the tracks with clips (one undo step); Draw on a locked track places its content; Unlock
    /// all frees them.
    #[test]
    fn lock_and_unlock_all() {
        let mut app = app();
        app.menu_item(TOOLS_MENU, index(TOOLS_MENU, LOCK_ALL));
        let locked: Vec<u16> = (0..pl::TRACKS).filter(|&t| pl::track_info(&app.state, t).is_some_and(|x| x.locked)).collect();
        assert_eq!(locked, vec![0, 3]);
        assert_eq!(app.state.message.as_deref(), Some("Locked 2 tracks to their content"));
        // A locked track draws its own content, whatever the current source is.
        crate::patterns::pat_mut(&mut app.state, 2).name = Some("Other".into());
        app.state.playlist.source = Some(ClipSource::Pattern(2));
        let i = pl::place(&mut app.state, 0, 768);
        assert_eq!(pl::clips(&app.state)[i].source, ClipSource::Pattern(1));
        let i = pl::place(&mut app.state, 1, 768);
        assert_eq!(pl::clips(&app.state)[i].source, ClipSource::Pattern(2), "an unlocked track takes the current source");
        app.menu_item(TOOLS_MENU, index(TOOLS_MENU, UNLOCK_ALL));
        assert!((0..pl::TRACKS).all(|t| !pl::track_info(&app.state, t).is_some_and(|x| x.locked)));
        // Undo brings the locks back.
        pl::run(&mut app, pl::Cmd::Undo);
        assert!(pl::track_info(&app.state, 3).is_some_and(|x| x.locked));
    }

    /// The track menu's Lock to content: on a track with clips it locks, and a header click picks its content.
    #[test]
    fn track_menu_lock() {
        let mut app = app();
        pl::run(&mut app, pl::Cmd::TrackLock(5));
        assert!(!pl::track_info(&app.state, 5).is_some_and(|x| x.locked), "nothing to lock to");
        pl::run(&mut app, pl::Cmd::TrackLock(3));
        assert!(pl::track_info(&app.state, 3).is_some_and(|x| x.locked));
        assert_eq!(pl::locked_source(&app.state, 3), Some(ClipSource::Pattern(1)));
        app.state.playlist.source = Some(ClipSource::Pattern(4));
        pl::press_header(&mut app, 3, false, (0.0, 0.0));
        assert_eq!(pl::source(&app.state), ClipSource::Pattern(1));
        pl::run(&mut app, pl::Cmd::TrackLock(3));
        assert!(!pl::track_info(&app.state, 3).is_some_and(|x| x.locked));
    }

    /// ADD > Automation clip: a new automation channel with no targets and its clip in the playlist.
    #[test]
    fn add_automation_clip() {
        let mut app = app();
        let n = app.state.project.channels.len();
        app.menu_item(ADD_MENU, index(ADD_MENU, AUTOMATION_CLIP));
        assert_eq!(app.state.project.channels.len(), n + 1);
        let ch = app.state.project.channels[n].index;
        let a = app.state.project.automation(ch).expect("automation data");
        assert!(a.targets.is_empty() && a.points.len() == 2, "unlinked, two points");
        assert!(pl::clips(&app.state).iter().any(|c| c.source == ClipSource::Channel(ch)));
        assert_eq!(pl::source_kind(&app.state, ClipSource::Channel(ch)), pl::PickKind::Automation);
        assert!(app.state.shown(Win::Playlist));
    }
}
