//! Text fields: a line, or a box of lines, of editable text with a caret. [`Field`] holds the text and the caret and
//! does the editing (typing, Backspace / Delete, the arrows, Home / End, Enter in a box of lines); [`paint`] draws a
//! field sunken like the Find boxes, with the caret while it has the keyboard. As with the other widgets, hit-testing
//! and the routing of keys stay with the caller.
// spec: ui/settings.md "Info" (the project credits are text fields)

use os_theme::Palette;

use super::{sunken, State};
use crate::{Canvas, Font, Rect};

/// The text of a field being edited and its caret.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Field {
    pub text: String,
    /// Byte offset of the caret in `text`, always on a character boundary.
    pub caret: usize,
    /// Enter starts a new line (a comments box). A one-line field keeps no line breaks.
    pub multiline: bool,
}

impl Field {
    /// A field showing `text` with the caret at its end.
    pub fn new(text: &str, multiline: bool) -> Field {
        let mut f = Field { text: String::new(), caret: 0, multiline };
        f.insert(text);
        f
    }

    /// Types `s` at the caret. "\r\n" and "\r" count as line breaks; line breaks become spaces in a one-line field and
    /// other control characters are dropped (pasted text).
    pub fn insert(&mut self, s: &str) {
        let s = s.replace("\r\n", "\n").replace('\r', "\n");
        let s: String = s
            .chars()
            .filter_map(|c| match c {
                '\n' if self.multiline => Some('\n'),
                '\n' | '\t' => Some(' '),
                c if c.is_control() => None,
                c => Some(c),
            })
            .collect();
        self.text.insert_str(self.caret, &s);
        self.caret += s.len();
    }

    /// Enter: a new line in a box of lines; false (nothing typed) in a one-line field.
    pub fn newline(&mut self) -> bool {
        if !self.multiline {
            return false;
        }
        self.insert("\n");
        true
    }

    /// Deletes the character before the caret; false at the start.
    pub fn backspace(&mut self) -> bool {
        let Some(p) = self.prev() else { return false };
        self.text.replace_range(p..self.caret, "");
        self.caret = p;
        true
    }

    /// Deletes the character after the caret; false at the end.
    pub fn delete(&mut self) -> bool {
        let Some(n) = self.next() else { return false };
        self.text.replace_range(self.caret..n, "");
        true
    }

    pub fn left(&mut self) {
        self.caret = self.prev().unwrap_or(0);
    }

    pub fn right(&mut self) {
        self.caret = self.next().unwrap_or(self.text.len());
    }

    /// To the start of the caret's line.
    pub fn home(&mut self) {
        self.caret = self.line_start(self.caret);
    }

    /// To the end of the caret's line.
    pub fn end(&mut self) {
        self.caret = self.text[self.caret..].find('\n').map_or(self.text.len(), |i| self.caret + i);
    }

    /// One line up, at the same column (or the line's end when it is shorter); to the start on the first line.
    pub fn up(&mut self) {
        let start = self.line_start(self.caret);
        if start == 0 {
            self.caret = 0;
            return;
        }
        let col = self.text[start..self.caret].chars().count();
        let above = self.line_start(start - 1);
        self.caret = at_column(&self.text, above, start - 1, col);
    }

    /// One line down, at the same column; to the end on the last line.
    pub fn down(&mut self) {
        let start = self.line_start(self.caret);
        let col = self.text[start..self.caret].chars().count();
        let Some(nl) = self.text[self.caret..].find('\n').map(|i| self.caret + i) else {
            self.caret = self.text.len();
            return;
        };
        let below = nl + 1;
        let below_end = self.text[below..].find('\n').map_or(self.text.len(), |i| below + i);
        self.caret = at_column(&self.text, below, below_end, col);
    }

    /// The caret's line (from 0) and the byte offset of the caret within that line.
    pub fn caret_line(&self) -> (usize, usize) {
        let start = self.line_start(self.caret);
        (self.text[..start].matches('\n').count(), self.caret - start)
    }

    fn line_start(&self, at: usize) -> usize {
        self.text[..at].rfind('\n').map_or(0, |i| i + 1)
    }

    fn prev(&self) -> Option<usize> {
        self.text[..self.caret].char_indices().next_back().map(|(i, _)| i)
    }

    fn next(&self) -> Option<usize> {
        self.text[self.caret..].chars().next().map(|c| self.caret + c.len_utf8())
    }
}

/// The byte offset of column `col` (in characters) of the line `text[start..end]`, or `end` when it is shorter.
fn at_column(text: &str, start: usize, end: usize, col: usize) -> usize {
    text[start..end].char_indices().nth(col).map_or(end, |(i, _)| start + i)
}

/// Inner margin between the frame and the text.
const PAD: f32 = 6.0;

/// Draws a field showing `text` in `r`: one line centred, or (`lines`) lines from the top of a taller box. `caret`
/// (byte offset in `text`) is drawn while the field has the keyboard, and the text scrolls to keep it in view; a
/// disabled field draws its text dimmed.
pub fn paint(c: &mut Canvas, t: &Palette, r: Rect, text: &str, caret: Option<usize>, font: Font, lines: bool, st: State) {
    let well = if caret.is_some() {
        t.s(0x141516)
    } else if st.hover && !st.disabled {
        t.s(0x1d1f21)
    } else {
        t.s(0x191b1c)
    };
    sunken(c, t, r, well);
    let lh = c.line_height(font);
    let col = if st.disabled { t.text_dim } else { t.text };
    let inner = r.inset(2.0);
    c.push_clip(inner);
    let w = r.w - 2.0 * PAD;
    // The caret's line and its x from the line's start.
    let cur = caret.map(|at| {
        let at = at.min(text.len());
        let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
        (text[..start].matches('\n').count(), at - start)
    });
    if lines {
        let shown = (((r.h - 2.0 * PAD) / lh).floor() as usize).max(1);
        let first = cur.map_or(0, |(line, _)| line.saturating_sub(shown - 1));
        for (i, line) in text.split('\n').enumerate().skip(first).take(shown) {
            let y = r.y + PAD - 2.0 + (i - first) as f32 * lh;
            let shift = cur.filter(|&(l, _)| l == i).map_or(0.0, |(_, b)| (c.text_width(font, &line[..b.min(line.len())]) - w + 2.0).max(0.0));
            c.text(font, r.x + PAD - shift, y, col, line);
            if let Some((_, b)) = cur.filter(|&(l, _)| l == i) {
                let x = r.x + PAD - shift + c.text_width(font, &line[..b.min(line.len())]);
                c.fill_rect(Rect::new(x.round(), y + 1.0, 1.0, lh - 2.0), t.text);
            }
        }
    } else {
        let y = (r.cy() - lh / 2.0).round();
        let line = text.split('\n').next().unwrap_or("");
        match cur {
            Some((_, b)) => {
                let cx = c.text_width(font, &line[..b.min(line.len())]);
                let shift = (cx - w + 2.0).max(0.0);
                c.text(font, r.x + PAD - shift, y, col, line);
                c.fill_rect(Rect::new((r.x + PAD - shift + cx).round(), r.y + 4.0, 1.0, r.h - 8.0), t.text);
            }
            None => c.text_clip(font, r.x + PAD, y, w, col, line),
        }
    }
    c.pop_clip();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_deleting_move_the_caret() {
        let mut f = Field::new("Song", false);
        assert_eq!(f.caret, 4);
        f.insert(" two");
        assert_eq!(f.text, "Song two");
        f.left();
        f.left();
        f.left();
        f.backspace();
        assert_eq!((f.text.as_str(), f.caret), ("Songtwo", 4));
        f.delete();
        assert_eq!(f.text, "Songwo");
        f.home();
        assert!(!f.backspace(), "nothing before the start");
        f.end();
        assert!(!f.delete(), "nothing after the end");
        f.right();
        assert_eq!(f.caret, 6);
    }

    #[test]
    fn characters_are_whole_and_controls_dropped() {
        let mut f = Field::new("Café", false);
        f.backspace();
        assert_eq!(f.text, "Caf");
        f.insert("é\u{7}");
        f.left();
        assert_eq!(f.caret, 3, "left steps over the whole é");
        f.insert("a\r\nb\tc");
        assert_eq!(f.text, "Cafa b cé", "a one-line field keeps no line breaks");
        assert!(!f.newline());
    }

    #[test]
    fn lines_edit_and_move_by_column() {
        let mut f = Field::new("first line\nab\nthird", true);
        assert_eq!(f.caret_line(), (2, 5));
        f.up();
        assert_eq!(f.caret_line(), (1, 2), "short line: its end");
        f.up();
        assert_eq!(f.caret_line(), (0, 2));
        f.up();
        assert_eq!(f.caret, 0);
        f.down();
        f.down();
        f.down();
        assert_eq!(f.caret, f.text.len());
        f.home();
        assert!(f.newline());
        assert_eq!(f.text, "first line\nab\n\nthird");
        f.up();
        f.insert("x\r\ny");
        assert_eq!(f.text, "first line\nab\nx\ny\nthird");
        assert_eq!(f.caret_line(), (3, 1));
        f.end();
        f.backspace();
        f.backspace();
        assert_eq!(f.text, "first line\nab\nx\nthird", "Backspace joins lines");
    }

    #[test]
    fn fields_paint_without_panics() {
        let Ok(mut c) = Canvas::new(200, 120, 1.0) else { return };
        let t = Palette::default();
        let long = "a long line of text that does not fit the field";
        for (caret, st) in [(None, State::NORMAL), (Some(3), State::hover(true)), (Some(long.len()), State::NORMAL), (None, State::NORMAL.with_disabled(true))] {
            paint(&mut c, &t, Rect::new(2.0, 2.0, 120.0, 22.0), long, caret, Font::new(13), false, st);
            paint(&mut c, &t, Rect::new(2.0, 30.0, 120.0, 60.0), "one\ntwo\nthree\nfour\nfive", caret.map(|c| c.min(17)), Font::new(13), true, st);
        }
    }
}
