//! The piano roll's Note properties dialog (toolbar "Note properties", or a double click on a note): every property of
//! one note, each with − / + buttons, a click on the value toggling the switches and a right-click putting the default
//! back. Opened from the toolbar with no note selected it edits what new notes get instead (velocity, pan, release,
//! cutoff, resonance, fine pitch, colour, slide). Editing a selected note changes the whole selection.
// spec: ui/piano-roll.md §5 (notes), §6 (note colours / MIDI channel), §8 (Draw tool), §12 (note properties);
// flp/events/patterns.md (note record fields)

use os_model::Note;

use crate::app::State;
use crate::piano::{self, Lane};
use crate::views::piano::TOP_KEY;

/// What the dialog edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A note of the current pattern, by index.
    Note(usize),
    /// The properties new notes get (`PianoState::template`).
    NewNotes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Props {
    pub target: Target,
    pub pattern: u16,
    /// An undo snapshot was taken since the dialog opened (one per opening).
    snapped: bool,
}

/// The rows of the dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Key,
    Position,
    Length,
    Velocity,
    Pan,
    Release,
    Cutoff,
    Resonance,
    FinePitch,
    Repeat,
    Color,
    Slide,
}

impl Field {
    pub const ALL: [Field; 12] = [
        Field::Key,
        Field::Position,
        Field::Length,
        Field::Velocity,
        Field::Pan,
        Field::Release,
        Field::Cutoff,
        Field::Resonance,
        Field::FinePitch,
        Field::Repeat,
        Field::Color,
        Field::Slide,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Field::Key => "Key",
            Field::Position => "Position",
            Field::Length => "Length",
            Field::Velocity => "Velocity",
            Field::Pan => "Pan",
            Field::Release => "Release",
            Field::Cutoff => "Filter cutoff",
            Field::Resonance => "Filter resonance",
            Field::FinePitch => "Fine pitch",
            Field::Repeat => "Repeat",
            Field::Color => "Color / MIDI channel",
            Field::Slide => "Slide",
        }
    }

    /// The event panel lane that edits the same property.
    fn lane(self) -> Option<Lane> {
        Some(match self {
            Field::Velocity => Lane::Velocity,
            Field::Pan => Lane::Pan,
            Field::Release => Lane::Release,
            Field::Cutoff => Lane::Cutoff,
            Field::Resonance => Lane::Resonance,
            Field::FinePitch => Lane::FinePitch,
            Field::Repeat => Lane::Repeat,
            _ => return None,
        })
    }

    /// Place and length belong to a note, not to the new-note properties.
    pub fn for_new_notes(self) -> bool {
        !matches!(self, Field::Key | Field::Position | Field::Length)
    }

    /// An on / off switch: a click on the value toggles it.
    pub fn is_switch(self) -> bool {
        self == Field::Slide
    }
}

/// The dialog's rows for `target`.
pub fn fields(target: Target) -> Vec<Field> {
    Field::ALL.into_iter().filter(|f| target != Target::NewNotes || f.for_new_notes()).collect()
}

pub fn get(n: &Note, f: Field) -> i64 {
    match f {
        Field::Key => i64::from(n.key),
        Field::Position => i64::from(n.position),
        Field::Length => i64::from(n.length),
        Field::Color => i64::from(piano::color_of(n)),
        Field::Slide => i64::from(n.slide()),
        _ => f.lane().map_or(0, |l| i64::from(l.get(n))),
    }
}

/// Lowest and highest value of `f`.
fn range(f: Field) -> (i64, i64) {
    match f {
        Field::Key => (0, i64::from(TOP_KEY)),
        Field::Position => (0, i64::from(u32::MAX / 2)),
        Field::Length => (1, i64::from(u32::MAX / 2)),
        Field::Color => (0, 15),
        Field::Slide => (0, 1),
        _ => f.lane().map_or((0, 0), |l| (0, l.max() as i64)),
    }
}

pub fn set(n: &mut Note, f: Field, v: i64) {
    let (lo, hi) = range(f);
    let v = v.clamp(lo, hi);
    match f {
        Field::Key => n.key = v as u16,
        Field::Position => n.position = v as u32,
        Field::Length => n.length = v as u32,
        Field::Color => piano::set_color(n, v as u8),
        Field::Slide => piano::set_slide(n, v != 0),
        _ => {
            if let Some(l) = f.lane() {
                l.set(n, v as u8);
            }
        }
    }
}

/// What one − / + click adds: a step (1/16 note) for place and length, one unit otherwise.
pub fn unit(f: Field, ppq: u16) -> i64 {
    match f {
        Field::Position | Field::Length => i64::from(ppq.max(4) / 4),
        _ => 1,
    }
}

/// The value as shown: key names, bars:steps:ticks, percentages and centred values as the event panel names them.
pub fn text(n: &Note, f: Field, ppq: u16) -> String {
    let v = get(n, f);
    let ppq = i64::from(ppq.max(4));
    let step = ppq / 4;
    match f {
        Field::Key => crate::views::piano::key_name(v as i32),
        Field::Position => format!("{}:{:02}:{:02}", v / (ppq * 4) + 1, v % (ppq * 4) / step + 1, v % step),
        Field::Length if n.length == 0 => "step".into(),
        Field::Length => format!("{}:{:02}", v / step, v % step),
        Field::Velocity | Field::Release => format!("{}%", (v * 100 + 64) / 128),
        Field::Pan => match v - 64 {
            0 => "Centered".into(),
            d if d < 0 => format!("{}% left", (-d * 100 + 32) / 64),
            d => format!("{}% right", (d * 100 + 32) / 64),
        },
        Field::Cutoff | Field::Resonance => format!("{:+}", v - 128),
        Field::FinePitch => format!("{:+} cents", v - 120),
        Field::Repeat if v == 0 => "Off".into(),
        Field::Repeat => format!("{v}"),
        Field::Color => format!("{}", v + 1),
        Field::Slide => if v != 0 { "On" } else { "Off" }.into(),
    }
}

/// A new note's value (the step defaults, patterns.md); a note's own key, place and length stay as they are.
fn default(n: &Note, f: Field) -> i64 {
    match f {
        Field::Key | Field::Position | Field::Length => get(n, f),
        _ => get(&Note::step(0, 0), f),
    }
}

/// Open the dialog on `target`.
pub fn open(s: &mut State, target: Target) {
    s.piano.props = Some(Props { target, pattern: s.pattern, snapped: false });
}

/// The note the dialog shows (the new-note properties for `NewNotes`); `None` when its note is gone (the dialog then
/// closes on the next edit).
pub fn shown(s: &State) -> Option<Note> {
    let p = s.piano.props?;
    match p.target {
        Target::NewNotes => Some(s.piano.template),
        Target::Note(i) => {
            let ch = piano::channel(s);
            piano::notes(s).get(i).filter(|n| p.pattern == s.pattern && Some(n.channel) == ch).copied()
        }
    }
}

/// − / + on a row: `dir` units.
pub fn step(s: &mut State, f: Field, dir: i64) {
    let ppq = s.project.ppq;
    edit(s, f, |n| get(n, f) + dir * unit(f, ppq));
}

/// A click on a value: switches toggle.
pub fn click(s: &mut State, f: Field) {
    if f.is_switch() {
        edit(s, f, |n| 1 - get(n, f));
    }
}

/// Right-click on a row: back to the default.
pub fn reset(s: &mut State, f: Field) {
    edit(s, f, |n| default(n, f));
}

/// Set `f` of the dialog's note to `value(note)`. A selected note takes the selection with it: properties are set to
/// the same value, key and place move every note by the same amount, lengths change by the same amount.
fn edit(s: &mut State, f: Field, value: impl Fn(&Note) -> i64) {
    let Some(p) = s.piano.props else { return };
    let Some(n) = shown(s) else {
        s.piano.props = None;
        return;
    };
    let v = value(&n).clamp(range(f).0, range(f).1);
    let Target::Note(i) = p.target else {
        set(&mut s.piano.template, f, v);
        return;
    };
    let old = get(&n, f);
    if v == old && !(f == Field::Length && n.length == 0) {
        return;
    }
    let sel = s.piano.selection(s.pattern, piano::channel(s));
    let idx = if sel.contains(&i) { sel } else { vec![i] };
    let relative = matches!(f, Field::Key | Field::Position | Field::Length);
    // A move that would take any note out of range is refused as a whole.
    let all = piano::notes(s);
    if relative && idx.iter().filter_map(|&j| all.get(j)).any(|m| get(m, f) + v - old < range(f).0) {
        return;
    }
    if !p.snapped {
        piano::snapshot(s);
        if let Some(p) = s.piano.props.as_mut() {
            p.snapped = true;
        }
    }
    let pat = piano::pattern_mut(s);
    for &j in &idx {
        if let Some(m) = pat.notes.get_mut(j) {
            let to = if relative { get(m, f) + v - old } else { v };
            set(m, f, to);
        }
    }
    let edited = pat.notes[i];
    s.piano.dirty = true;
    piano::finish_edit(s);
    // The note may have moved in storage order: follow it.
    if let Some(j) = piano::notes(s).iter().position(|m| *m == edited) {
        if let Some(p) = s.piano.props.as_mut() {
            p.target = Target::Note(j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    fn app() -> App {
        let mut a = App::new(None);
        a.state.selected_channel = 4;
        a
    }

    #[test]
    fn values_read_and_clamp() {
        let mut n = Note::step(4, 0);
        assert_eq!(text(&n, Field::Pan, 96), "Centered");
        assert_eq!(text(&n, Field::Velocity, 96), "78%");
        set(&mut n, Field::Velocity, 999);
        assert_eq!(n.velocity, 128);
        set(&mut n, Field::Color, 20);
        assert_eq!(piano::color_of(&n), 15);
        set(&mut n, Field::Slide, 1);
        assert!(n.slide());
        assert_eq!(n.flags & 0x4000, 0x4000, "other flags are kept");
        n.position = 96 * 4 + 24 * 2 + 5;
        assert_eq!(text(&n, Field::Position, 96), "2:03:05");
        assert!(!fields(Target::NewNotes).contains(&Field::Key));
        assert_eq!(fields(Target::Note(0)).len(), Field::ALL.len());
    }

    /// The dialog edits a note (and the selection with it), takes one undo snapshot, follows the note when its place
    /// changes the storage order and puts defaults back on a right-click.
    #[test]
    fn edits_note_and_selection() {
        let mut a = app();
        let s = &mut a.state;
        let i = piano::add_note(s, 4, 0, 60);
        let j = piano::add_note(s, 4, 96, 64);
        s.piano.select(s.pattern, 4, vec![i, j]);
        open(s, Target::Note(i));
        for _ in 0..3 {
            step(s, Field::Velocity, 1);
        }
        assert_eq!((piano::notes(s)[0].velocity, piano::notes(s)[1].velocity), (103, 103), "same value for the selection");
        step(s, Field::Key, 2);
        assert_eq!((piano::notes(s)[0].key, piano::notes(s)[1].key), (62, 66), "keys move together");
        click(s, Field::Slide);
        assert!(piano::notes(s).iter().all(|n| n.slide()));
        reset(s, Field::Velocity);
        assert_eq!(piano::notes(s)[0].velocity, 100);
        // Moving the first note past the second changes the order: the dialog follows it.
        s.piano.clear_selection();
        for _ in 0..5 {
            step(s, Field::Position, 1);
        }
        assert_eq!(piano::notes(s)[1].position, 120);
        assert_eq!(s.piano.props.map(|p| p.target), Some(Target::Note(1)));
        assert!(piano::undo(s), "one snapshot for the opening");
        assert_eq!(piano::notes(s)[0].velocity, 100);
        assert_eq!(piano::notes(s)[0].key, 60);
    }

    /// From the toolbar with nothing selected: the properties of new notes.
    #[test]
    fn new_note_properties() {
        let mut a = app();
        let s = &mut a.state;
        open(s, Target::NewNotes);
        step(s, Field::Color, 3);
        click(s, Field::Slide);
        step(s, Field::Pan, -10);
        let i = piano::add_note(s, 4, 0, 60);
        let n = piano::notes(s)[i];
        assert_eq!((piano::color_of(&n), n.slide(), n.pan), (3, true, 54));
        assert!(!piano::undo(s), "no note changed: nothing to undo");
    }
}
