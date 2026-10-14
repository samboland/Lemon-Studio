//! The toolbar's tempo display: a drag up / down changes the tempo by whole BPM, with Ctrl held by hundredths (the
//! decimals); a right-click opens a box to type a tempo in (the pattern name box, `BoxTarget::Tempo`). Lemon Studio
//! choice: FL fine-tunes by dragging the decimals; Lemon by Ctrl, as its knobs do.
// spec: ui/main-window.md §2

use crate::app::App;
use crate::patterns::{open_name_box, BoxTarget};

/// The tempo box's range (BPM).
pub const MIN: f64 = 10.0;
pub const MAX: f64 = 522.0;
/// BPM per pixel of a fine (Ctrl) drag.
pub const FINE: f64 = 0.01;
/// Where the typing box opens: under the tempo display.
pub const BOX_AT: (f32, f32) = (517.0, 38.0);

/// The tempo after a drag of `dy` pixels up from a press at tempo `t0`: whole BPM, or hundredths when `fine`; kept to
/// three decimals (what the display shows) and in range.
pub fn dragged(t0: f64, dy: f32, fine: bool) -> f64 {
    let t = if fine { t0 + f64::from(dy.round()) * FINE } else { t0 + f64::from(dy.round()) };
    clamp(t)
}

/// In range, three decimals.
pub fn clamp(t: f64) -> f64 {
    (t.clamp(MIN, MAX) * 1000.0).round() / 1000.0
}

/// A typed tempo: "128", "127.5", "127,5" (a comma for the decimal point too), spaces and "bpm" ignored. None when it is
/// no number.
pub fn parse(text: &str) -> Option<f64> {
    let t = text.trim().to_lowercase();
    let t = t.trim_end_matches("bpm").trim().replace(',', ".");
    t.parse::<f64>().ok().filter(|v| v.is_finite()).map(clamp)
}

/// The typing box's text when it opens: the tempo without trailing zeros.
pub fn text(t: f64) -> String {
    let s = format!("{t:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The box was accepted with `text` (`patterns::accept`): a number sets the tempo, anything else leaves it.
pub fn accept(s: &mut crate::app::State, text: &str) {
    if let Some(t) = parse(text) {
        if s.tempo != t {
            s.tempo = t;
            s.piano.dirty = true;
        }
    }
}

/// Right-click on the tempo display: the typing box.
pub fn type_in(app: &mut App) {
    open_name_box(&mut app.state, BoxTarget::Tempo, BOX_AT);
    app.invalidate();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Action;

    #[test]
    fn drag_whole_and_fine() {
        assert_eq!(dragged(130.0, 5.0, false), 135.0);
        assert_eq!(dragged(130.5, -2.4, false), 128.5);
        assert_eq!(dragged(130.0, 5.0, true), 130.05);
        assert_eq!(dragged(130.0, -120.0, true), 128.8);
        assert_eq!(dragged(12.0, -50.0, false), MIN);
        assert_eq!(dragged(520.0, 50.0, false), MAX);
    }

    #[test]
    fn typed_tempos() {
        assert_eq!(parse("128"), Some(128.0));
        assert_eq!(parse(" 127,5 "), Some(127.5));
        assert_eq!(parse("140 BPM"), Some(140.0));
        assert_eq!(parse("99.12345"), Some(99.123));
        assert_eq!(parse("2000"), Some(MAX));
        assert_eq!(parse("fast"), None);
        assert_eq!(parse("NaN"), None);
        assert_eq!(text(130.0), "130");
        assert_eq!(text(127.5), "127.5");
    }

    /// Ctrl+drag on the tempo display changes the decimals, a plain drag whole BPM; right-click types a tempo in.
    #[test]
    fn toolbar_tempo_drag_and_typing() {
        use winit::keyboard::{Key, NamedKey};
        let Ok(mut c) = os_ui::Canvas::new(1920, 1080, 1.0) else { return };
        let mut app = App::new(None);
        app.paint(&mut c, None);
        let r = app.hits.iter().find(|h| h.action == Action::Tempo).expect("the tempo display").rect;
        // Plain drag: 3 px up = 3 BPM.
        app.mouse_move(r.cx(), r.cy());
        app.mouse_down();
        app.mouse_move(r.cx(), r.cy() - 3.0);
        app.mouse_up();
        assert_eq!(app.state.tempo, 133.0);
        // Ctrl+drag: 20 px down = 0.2 BPM.
        app.ctrl = true;
        app.mouse_move(r.cx(), r.cy());
        app.mouse_down();
        app.mouse_move(r.cx(), r.cy() + 20.0);
        app.mouse_up();
        app.ctrl = false;
        assert!((app.state.tempo - 132.8).abs() < 1e-9, "{}", app.state.tempo);
        // Right-click: the box, with the tempo selected; typing replaces it, Enter sets it.
        app.mouse_move(r.cx(), r.cy());
        app.mouse_down_right();
        let b = app.state.pat_ui.name_box.as_ref().expect("right-click opens the typing box");
        assert_eq!((b.target, b.text.as_str()), (BoxTarget::Tempo, "132.8"));
        for ch in ["9", "6", ".", "5"] {
            app.key(&Key::Character(ch.into()), false, false);
        }
        app.key(&Key::Named(NamedKey::Enter), false, false);
        assert_eq!(app.state.tempo, 96.5);
        assert!(app.state.pat_ui.name_box.is_none());
        // Junk leaves the tempo; Escape cancels.
        type_in(&mut app);
        app.key(&Key::Character("x".into()), false, false);
        app.key(&Key::Named(NamedKey::Enter), false, false);
        assert_eq!(app.state.tempo, 96.5);
        type_in(&mut app);
        app.key(&Key::Character("7".into()), false, false);
        app.key(&Key::Named(NamedKey::Escape), false, false);
        assert_eq!(app.state.tempo, 96.5);
    }
}
