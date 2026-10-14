//! Settings > Project: data folder, time settings, advanced, playlist.
// spec: ui/settings.md "Project"

use super::{combo, g, gf, txt, Group, Row, SAct};
use crate::app::{App, State};

/// "Play truncated notes in clips" (spec: audio/song-structure.md): notes that start before a clip's window, or run
/// past its end, play the part inside it. Saved as event 30.
pub const TRUNCATED_CHECK: &str = "Play truncated notes in clips";
/// The time signature's steps (Lemon Studio choice: buttons that step the numerator by one and the denominator by
/// powers of two, in place of FL's two combos).
pub const NUM_DOWN: &str = "Numerator -";
pub const NUM_UP: &str = "Numerator +";
pub const DEN_DOWN: &str = "Denominator -";
pub const DEN_UP: &str = "Denominator +";

/// The checks and buttons of this page that work (`SAct::Check` / `SAct::Button` labels, handled in `act`).
pub const LIVE: &[&str] = &[TRUNCATED_CHECK, NUM_DOWN, NUM_UP, DEN_DOWN, DEN_UP];

/// Beats in a bar: 1 to 16 (Lemon Studio choice).
const NUMERATORS: std::ops::RangeInclusive<u16> = 1..=16;
/// The note a beat is: half, quarter, eighth, sixteenth (Lemon Studio choice).
const DENOMINATORS: [u16; 4] = [2, 4, 8, 16];

/// The project's time signature (events 17 and 18); 4/4 when the file has none.
pub fn time_signature(s: &State) -> (u16, u16) {
    s.project.time_signature.unwrap_or((4, 4))
}

/// Whether "play truncated notes" is on; off when the file does not say (as the engine plays it).
pub fn play_truncated(s: &State) -> bool {
    s.project.play_truncated.unwrap_or(false)
}

/// The Project page's groups (spec §4).
pub fn page(s: &State) -> Vec<Group> {
    use Row::*;
    let (n, d) = time_signature(s);
    let note = match d {
        1 => "whole",
        2 => "half",
        4 => "quarter",
        8 => "eighth",
        16 => "sixteenth",
        _ => "",
    };
    let beats = if note.is_empty() { format!("{n} beats per bar") } else { format!("{n} {note} notes per bar") };
    vec![
        g("Data folder", vec![
            combo("Don't use a data folder", 306.0, ""),
            txt("(none)"),
            combo("When saving unsaved projects only", 444.0, "Ask for data folder"),
            Off("Copy used samples to the project folder", false),
        ]),
        g("Time settings", vec![
            combo(&s.project.ppq.to_string(), 44.0, "Timebase (PPQ)"),
            combo("Set as time signature", 196.0, ""),
            txt(format!("Time signature {n}/{d}: {beats}")),
            Buttons(vec![(NUM_DOWN, 112.0), (NUM_UP, 112.0), (DEN_DOWN, 112.0), (DEN_UP, 112.0)]),
        ]),
        gf("Advanced", true, vec![
            combo("Circular", 82.0, "Panning law"),
            Check(TRUNCATED_CHECK, play_truncated(s)),
            Check("Fast declick for cut groups", false),
        ]),
        gf("Playlist", false, vec![Check2("Automatic crossfades", true, "Enable automatic crossfades in new projects", true)]),
    ]
}

/// The time signature after `step` (one of the four step buttons), or None when it is already at that end. A
/// denominator that is not one of the steps moves to the nearest step in that direction.
pub fn stepped(sig: (u16, u16), step: &str) -> Option<(u16, u16)> {
    let (n, d) = sig;
    let next = match step {
        NUM_DOWN => (n.saturating_sub(1).max(*NUMERATORS.start()), d),
        NUM_UP => ((n + 1).clamp(*NUMERATORS.start(), *NUMERATORS.end()), d),
        DEN_DOWN => (n, DENOMINATORS.iter().rev().copied().find(|&v| v < d).unwrap_or(d)),
        DEN_UP => (n, DENOMINATORS.iter().copied().find(|&v| v > d).unwrap_or(d)),
        _ => return None,
    };
    (next != sig).then_some(next)
}

/// Sets the project's time signature: bars (pattern lengths, the song's end) and the metronome's accent follow it at
/// once, also while playing (spec: audio/timing.md "Settings", ui/metronome.md).
pub fn set_time_signature(app: &mut App, sig: (u16, u16)) {
    app.state.project.time_signature = Some(sig);
    app.transport.update(&crate::rack::play_project(&app.state));
    app.state.message = Some(format!("Time signature {}/{}", sig.0, sig.1));
}

/// This page's working controls (`LIVE`).
pub(crate) fn act(app: &mut App, a: &SAct) -> bool {
    match *a {
        SAct::Check(TRUNCATED_CHECK) => {
            let on = !play_truncated(&app.state);
            app.state.project.play_truncated = Some(on);
            app.transport.update(&crate::rack::play_project(&app.state));
            app.state.message = Some(format!("Play truncated notes in clips: {}", if on { "on" } else { "off" }));
        }
        SAct::Button(k @ (NUM_DOWN | NUM_UP | DEN_DOWN | DEN_UP)) => match stepped(time_signature(&app.state), k) {
            Some(sig) => set_time_signature(app, sig),
            None => {
                let (n, d) = time_signature(&app.state);
                app.state.message = Some(format!("Time signature {n}/{d}: as far as it goes"));
            }
        },
        _ => return false,
    }
    app.invalidate();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Action;
    use os_ui::{Canvas, Rect};

    fn hit_rect(app: &App, a: &Action) -> Rect {
        app.hits.iter().rev().find(|h| &h.action == a).map(|h| h.rect).unwrap_or_else(|| panic!("no hit for {a:?}"))
    }

    fn click(app: &mut App, c: &mut Canvas, a: &Action) {
        let r = hit_rect(app, a);
        app.mouse_move(r.cx(), r.cy());
        app.mouse_down();
        app.mouse_up();
        app.paint(c, None);
    }

    #[test]
    fn signature_steps_stay_in_range() {
        assert_eq!(stepped((4, 4), NUM_UP), Some((5, 4)));
        assert_eq!(stepped((4, 4), NUM_DOWN), Some((3, 4)));
        assert_eq!(stepped((1, 4), NUM_DOWN), None);
        assert_eq!(stepped((16, 4), NUM_UP), None);
        assert_eq!(stepped((6, 4), DEN_UP), Some((6, 8)));
        assert_eq!(stepped((6, 16), DEN_UP), None);
        assert_eq!(stepped((6, 2), DEN_DOWN), None);
        assert_eq!(stepped((5, 3), DEN_DOWN), Some((5, 2)), "an odd denominator goes to the next step");
        assert_eq!(stepped((5, 3), DEN_UP), Some((5, 4)));
        assert_eq!(stepped((4, 4), "Other"), None);
    }

    /// The time signature buttons and "Play truncated notes in clips" work (no longer greyed out), change the project
    /// and the project saves them: a reopened file has them (events 17, 18 and 30).
    #[test]
    fn time_signature_and_truncation_are_set_saved_and_reopened() {
        let Ok(mut c) = Canvas::new(1920, 1080, 1.0) else { return };
        let mut app = App::new(None);
        app.open_settings(super::super::PROJECT);
        app.paint(&mut c, None);
        for k in [NUM_DOWN, NUM_UP, DEN_DOWN, DEN_UP] {
            assert!(!app.hits.iter().any(|h| h.action == Action::Placeholder(k)), "{k} is not greyed out");
        }
        assert_eq!(time_signature(&app.state), (4, 4));
        click(&mut app, &mut c, &Action::Settings(SAct::Button(NUM_UP)));
        click(&mut app, &mut c, &Action::Settings(SAct::Button(NUM_UP)));
        click(&mut app, &mut c, &Action::Settings(SAct::Button(DEN_UP)));
        assert_eq!(app.state.project.time_signature, Some((6, 8)));
        assert_eq!(app.state.message.as_deref(), Some("Time signature 6/8"));
        // Advanced is folded by default.
        click(&mut app, &mut c, &Action::Settings(SAct::Group("Advanced")));
        assert!(!app.hits.iter().any(|h| h.action == Action::Placeholder(TRUNCATED_CHECK)));
        assert!(app.hits.iter().any(|h| h.action == Action::Placeholder("Panning law")), "no pan law choice yet");
        let was = play_truncated(&app.state);
        click(&mut app, &mut c, &Action::Settings(SAct::Check(TRUNCATED_CHECK)));
        assert_eq!(app.state.project.play_truncated, Some(!was));
        assert!(app.unsaved());
        let path = std::env::temp_dir().join(format!("lemon-time-signature-{}.flp", std::process::id()));
        app.save_to(path.clone());
        assert!(app.state.message.as_deref().is_some_and(|m| m.starts_with("Saved")), "{:?}", app.state.message);
        let mut back = App::new(None);
        back.open(path.clone());
        let _ = std::fs::remove_file(&path);
        assert_eq!((back.state.project.time_signature, back.state.project.play_truncated), (Some((6, 8)), Some(!was)));
        // Edited again: the reopened file's events change in place.
        back.open_settings(super::super::PROJECT);
        back.paint(&mut c, None);
        click(&mut back, &mut c, &Action::Settings(SAct::Button(NUM_DOWN)));
        click(&mut back, &mut c, &Action::Settings(SAct::Button(DEN_DOWN)));
        back.save_to(path.clone());
        let mut again = App::new(None);
        again.open(path.clone());
        let _ = std::fs::remove_file(&path);
        assert_eq!(again.state.project.time_signature, Some((5, 4)));
    }

    /// The metronome accents the first beat of each bar of the signature set on this page.
    #[test]
    fn metronome_follows_the_new_signature() {
        let Ok(mut c) = Canvas::new(1920, 1080, 1.0) else { return };
        let mut app = App::new(None);
        app.open_settings(super::super::PROJECT);
        app.paint(&mut c, None);
        click(&mut app, &mut c, &Action::Settings(SAct::Button(NUM_DOWN)));
        assert_eq!(time_signature(&app.state), (3, 4));
        // What playback gets, with no notes: an empty bar of three beats looping, clicks only.
        let mut p = crate::rack::play_project(&app.state);
        p.patterns.iter_mut().for_each(|p| p.notes.clear());
        let (rate, ppq, bpm) = (48000, p.ppq, f64::from(app.state.tempo));
        let mut lib = os_engine::SampleLibrary::new(None);
        let mut e = os_engine::Engine::new(&p, &mut lib, rate, os_engine::PlayMode::Pattern(p.current_pattern.unwrap_or(1)));
        e.set_looping(true);
        let sw = crate::metronome::switch();
        sw.set_on(true);
        sw.set_sound(os_engine::metronome::Sound::Beep);
        e.set_metronome(sw);
        let beats = 6u64;
        let frames = os_engine::Clock::frame_of_tick(rate, ppq, bpm, u64::from(ppq) * beats) as usize;
        let mut out = vec![0.0; 2 * frames];
        e.process(&mut out);
        // The loudest sample of each beat's first 20 ms: accents are louder.
        let peaks: Vec<f32> = (0..beats)
            .map(|b| {
                let f = os_engine::Clock::frame_of_tick(rate, ppq, bpm, u64::from(ppq) * b) as usize;
                out[2 * f..(2 * (f + rate as usize / 50)).min(out.len())].iter().fold(0.0f32, |m, x| m.max(x.abs()))
            })
            .collect();
        let quiet = peaks.iter().copied().fold(f32::MAX, f32::min);
        assert!(quiet > 0.0, "every beat clicks: {peaks:?}");
        let accented: Vec<bool> = peaks.iter().map(|&v| v > quiet * 1.3).collect();
        assert_eq!(accented, [true, false, false, true, false, false], "{peaks:?}");
    }
}
