//! Settings > General: display, language, input, undo history, hints, system, updates, miscellaneous, advanced.
//! The working rows show and change `app_settings` (kept in `general.txt`).
// spec: ui/settings.md "General"

use super::{combo, dim, gf, pair, txt, Group, Row, SAct};
use crate::app::{App, State};
use crate::app_settings::{self as cfg, Names};

/// Note names and Black notes: FL's combos are radio checks here, as settings rows have no menus of their own yet
/// (Lemon Studio choice).
pub const NAMES_EN: &str = "Note names: C D E";
pub const NAMES_DO: &str = "Note names: Do Re Mi";
pub const SHARPS: &str = "Black notes: sharps - #";
pub const FLATS: &str = "Black notes: flats - b";
/// Maximum undo levels: a value with Fewer / More buttons stepping through `app_settings::UNDO_STEPS` (Lemon Studio
/// choice, for the same reason).
pub const FEWER: &str = "Fewer undo levels";
pub const MORE: &str = "More undo levels";
pub const AUTO_ZIP: &str = "Auto zip empty channels";
pub const LINKED: &str = "Auto select linked modules";
pub const AUTOMATION_AT_POS: &str = "Create automation clips at the playback position";
pub const FIRST_WITH_NOTES: &str = "Select first channel with notes when the pattern changes";

/// The checks and buttons of this page that work (`SAct::Check` / `SAct::Button` labels, handled in `act`).
pub const LIVE: &[&str] = &[NAMES_EN, NAMES_DO, SHARPS, FLATS, FEWER, MORE, AUTO_ZIP, LINKED, AUTOMATION_AT_POS, FIRST_WITH_NOTES];

/// The General page's groups (spec §4).
pub fn page(_s: &State) -> Vec<Group> {
    use Row::*;
    let g = cfg::get();
    vec![
        Group { note: " (requires a restart to take effect)", ..gf("Display", false, vec![
            combo("System", 64.0, "Main GUI scaling"),
            dim("Display PPI", txt("Define 1 inch / 2.54 cm or set display's PPI")),
            dim("Display PPI", Text("(96 PPI/advised scaling: 100%)".into(), true)),
            Slider("", 0.25),
            combo("Main", 64.0, "Pop-ups scaling"),
            combo("Main", 64.0, "Toolbars scaling"),
            combo("Off", 64.0, "Transparent windows"),
            combo("Entertain me", 126.0, "Animations"),
            Slider("Smoothing", 0.2),
            combo("Ultrasmooth", 126.0, "Animation refresh rate"),
            Check2("Force refreshes", true, "Transparent menus", true),
            Off("Thick lines", true),
            Check("High visibility", false),
            combo("Off", 146.0, "Colorful waveforms"),
        ]) },
        Group { note: " (requires a restart to take effect)", ..gf("Language", false, vec![
            combo("English (US) (en)", 176.0, "Language"),
            Check2(NAMES_EN, g.names == Names::English, NAMES_DO, g.names == Names::Solfege),
            Check2(SHARPS, !g.flats, FLATS, g.flats),
            Buttons(vec![("Update languages", 138.0)]),
        ]) },
        gf("Input", false, vec![
            combo("Multi-button mouse", 136.0, "Optimize for"),
            Buttons(vec![("Manage shortcuts", 212.0)]),
            Check("Click-and-hold & special gesture functions", false),
            Check("Use both mouse buttons in step sequencer", false),
            Slider("Vertical scrolling speed", 0.5),
            Slider("Horizontal scrolling speed", 0.5),
            Check("Invert pencil buttons", false),
        ]),
        gf("Undo history", false, vec![
            pair(Text(format!("Maximum undo levels: {}", g.undo_levels), false), Buttons(vec![(FEWER, 136.0), (MORE, 136.0)])),
            Check("Undo knob tweaks", true),
            Check("Alternate undo mode", true),
            Check("Put undone recordings in the recycle bin", true),
            Check("Show undo history in the browser when undoing changes", false),
            Check("Show recent changes at the top", false),
        ]),
        gf("Hints and warnings", false, vec![
            Buttons(vec![("Manage messages", 138.0)]),
            combo("100MB (default)", 110.0, "Project data size warning"),
        ]),
        gf("System", false, vec![Check("Associate project files with application", true)]),
        gf("Updates and privacy", false, vec![
            Check("Automatically check for updates", true),
            Check("Automatically download program updates", true),
            Check("Send usage data", false),
        ]),
        gf("Miscellaneous", false, vec![
            Check("Auto name channels", true),
            Check("Auto name effect slots", false),
            Check(AUTO_ZIP, g.auto_zip),
            Check(LINKED, g.linked),
            Check("Auto zoom in piano roll", false),
            Check("Small scrollbars in editors", false),
            Check2("Detach all new plugins", false, "Open detached plugins with the main window", false),
            Check("Disable typing keyboard priority by default", false),
            Check("Alternate meter scale", false),
            Check("Show startup splash screen", true),
            Check("Restore previous state after solo", false),
            Check("Hide plugin window toolbar by default", true),
            Check("Silent startup", false),
            Check("Monophonic painting in piano roll", false),
            Check(AUTOMATION_AT_POS, g.automation_at_pos),
            Check(FIRST_WITH_NOTES, g.first_with_notes),
            Check("Limit the number of open plugin editors", true),
            combo("Basic with limiter", 160.0, "Default template"),
            combo("Default template", 160.0, "Startup project"),
        ]),
        gf("Advanced", false, vec![
            Check("Don't limit windows to screen", false),
            Check("Fast sample preview", true),
            Check("Auto keep long audio on disk", true),
            Check2("Read sample tempo information", true, "Read sample root note", true),
            Check("Initialize controls automatically", true),
            Check("Force high performance power plan", true),
            Check("Threaded project sample loading", false),
            combo("System", 64.0, "Cursors scaling"),
            combo("Main", 64.0, "Legacy scaling"),
            combo("Show help in a web browser", 160.0, ""),
        ]),
    ]
}

/// This page's working controls (`LIVE`): each changes `app_settings`, which keeps it.
pub(crate) fn act(app: &mut App, a: &SAct) -> bool {
    match *a {
        SAct::Check(NAMES_EN) => cfg::set(|g| g.names = Names::English),
        SAct::Check(NAMES_DO) => cfg::set(|g| g.names = Names::Solfege),
        SAct::Check(SHARPS) => cfg::set(|g| g.flats = false),
        SAct::Check(FLATS) => cfg::set(|g| g.flats = true),
        SAct::Check(AUTO_ZIP) => cfg::set(|g| g.auto_zip = !g.auto_zip),
        SAct::Check(LINKED) => cfg::set(|g| g.linked = !g.linked),
        SAct::Check(AUTOMATION_AT_POS) => cfg::set(|g| g.automation_at_pos = !g.automation_at_pos),
        SAct::Check(FIRST_WITH_NOTES) => cfg::set(|g| g.first_with_notes = !g.first_with_notes),
        SAct::Button(k @ (FEWER | MORE)) => {
            let n = cfg::undo_step(cfg::undo_levels(), k == MORE);
            // Fewer levels: each editor drops its oldest snapshots at its next one.
            cfg::set(|g| g.undo_levels = n);
            app.state.message = Some(format!("Maximum undo levels: {n}"));
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Action;
    use crate::views::settings::GENERAL;
    use os_model::{Note, Pattern};
    use os_ui::Canvas;

    fn shown(app: &App, k: &'static str) -> bool {
        app.hits.iter().any(|h| h.action == Action::Settings(SAct::Check(k)) || h.action == Action::Settings(SAct::Button(k)))
    }

    fn placeholder(app: &App, k: &'static str) -> bool {
        app.hits.iter().any(|h| h.action == Action::Placeholder(k))
    }

    /// The live rows are clickable (no longer placeholders), change the settings, and keep them across a restart.
    #[test]
    fn general_rows_work_and_persist() {
        let Ok(mut c) = Canvas::new(1920, 1080, 1.0) else { return };
        let dir = std::env::temp_dir().join(format!("lemon-general-page-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        cfg::test_dir(Some(dir.clone()));
        cfg::forget();
        let mut app = App::new(None);
        app.open_settings(GENERAL);
        app.paint(&mut c, None);
        for k in [NAMES_EN, NAMES_DO, SHARPS, FLATS, FEWER, MORE] {
            assert!(shown(&app, k) && !placeholder(&app, k), "{k} works");
        }
        assert!(placeholder(&app, "Update languages"), "FL's language downloads stay greyed");
        app.state.settings.scroll = 5000.0;
        app.paint(&mut c, None);
        for k in [AUTO_ZIP, LINKED, AUTOMATION_AT_POS, FIRST_WITH_NOTES] {
            assert!(shown(&app, k) && !placeholder(&app, k), "{k} works");
        }
        for k in ["Auto name channels", "Small scrollbars in editors", "Monophonic painting in piano roll"] {
            assert!(placeholder(&app, k), "{k} stays greyed");
        }

        for k in [NAMES_DO, FLATS, AUTO_ZIP, LINKED, AUTOMATION_AT_POS, FIRST_WITH_NOTES] {
            app.act(Action::Settings(SAct::Check(k)));
        }
        app.act(Action::Settings(SAct::Button(FEWER)));
        assert_eq!(app.state.message.as_deref(), Some("Maximum undo levels: 50"));
        assert_eq!(crate::views::piano::key_name(61), "Reb5", "the piano roll's keys follow Note names / Black notes");
        assert_eq!(crate::rack::key_name(70), "Sib5");
        let want = cfg::General {
            undo_levels: 50,
            auto_zip: true,
            linked: false,
            automation_at_pos: false,
            first_with_notes: true,
            names: Names::Solfege,
            flats: true,
            autosave: 10,
        };
        assert_eq!(cfg::get(), want);
        // Quit and start again: read back from general.txt.
        drop(app);
        cfg::forget();
        let app = App::new(None);
        assert_eq!(cfg::get(), want);
        drop(app);
        cfg::test_dir(None);
        cfg::forget();
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Maximum undo levels caps the piano roll's and the playlist's undo history.
    #[test]
    fn undo_levels_cap_the_history() {
        cfg::test_dir(None);
        cfg::forget();
        let mut app = App::new(None);
        for _ in 0..3 {
            app.act(Action::Settings(SAct::Button(FEWER)));
        }
        assert_eq!(cfg::undo_levels(), 10);
        let s = &mut app.state;
        for _ in 0..30 {
            crate::piano::snapshot(s);
            crate::playlist::snapshot(s);
        }
        assert_eq!(crate::playlist::undo_len(s), 10);
        let mut n = 0;
        while crate::piano::undo(s) {
            n += 1;
        }
        assert_eq!(n, 10);
        cfg::forget();
    }

    /// Create automation clips at the playback position: on, the clip starts at the song position; off, at the start.
    #[test]
    fn automation_clips_start_where_the_setting_says() {
        cfg::test_dir(None);
        cfg::forget();
        let mut app = App::new(None);
        let s = &mut app.state;
        s.playlist.pos = 384;
        let tempo = os_model::target::TEMPO;
        crate::playlist::create_automation(s, tempo, "Tempo", 0.5);
        assert_eq!(crate::playlist::clips(s).last().unwrap().position, 384);
        cfg::set(|g| g.automation_at_pos = false);
        crate::playlist::create_automation(s, tempo, "Tempo", 0.5);
        assert_eq!(crate::playlist::clips(s).last().unwrap().position, 0);
        cfg::forget();
    }

    /// On a pattern change: the first channel with notes is selected, the empty channels zip; a new selected channel
    /// selects its mixer track (Auto select linked modules).
    #[test]
    fn pattern_changes_follow_the_settings() {
        cfg::test_dir(None);
        cfg::forget();
        let mut app = App::new(None);
        let clap = app.state.project.channels[1].index;
        let hat = app.state.project.channels[2].index;
        app.state.project.patterns.push(Pattern { number: 2, notes: vec![Note::step(clap, 0), Note::step(hat, 96)], ..Pattern::default() });
        app.tick();
        // Linked (on by default): selecting channel 4 selects its insert.
        app.state.selected_channel = 3;
        app.tick();
        assert_eq!(app.state.selected_mixer, app.state.project.channels[3].mixer_track);
        // Off by default: a pattern change leaves the selection and the zips.
        app.state.pattern = 2;
        app.tick();
        assert_eq!(app.state.selected_channel, 3);
        assert!(app.state.rack.zipped.is_empty());
        cfg::set(|g| {
            g.first_with_notes = true;
            g.auto_zip = true;
        });
        app.state.pattern = 1;
        app.tick();
        assert_eq!(app.state.rack.zipped.len(), 5, "pattern 1 is empty: every channel zips");
        app.state.pattern = 2;
        app.tick();
        assert_eq!(app.state.selected_channel, 1, "Clap: the first channel with notes");
        assert_eq!(app.state.selected_mixer, app.state.project.channels[1].mixer_track);
        assert_eq!(app.state.rack.zipped.iter().copied().collect::<Vec<_>>(), [0, 3, 4]);
        cfg::forget();
    }
}
