//! Scale and chord helpers of the piano roll (toolbar "Scale and chord helpers"): key / scale highlighting of the grid
//! rows and the keyboard, and the chord stamp the Draw tool places instead of a single note.
// spec: ui/piano-roll.md §2 (toolbar helpers), §3 (grid rows), §8 (Draw tool)

/// Scales the rows can be highlighted for, as semitone steps from the root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Major,
    Minor,
    HarmonicMinor,
    MelodicMinor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
    MajorPentatonic,
    MinorPentatonic,
    Blues,
}

impl Scale {
    pub const ALL: [Scale; 12] = [
        Scale::Major,
        Scale::Minor,
        Scale::HarmonicMinor,
        Scale::MelodicMinor,
        Scale::Dorian,
        Scale::Phrygian,
        Scale::Lydian,
        Scale::Mixolydian,
        Scale::Locrian,
        Scale::MajorPentatonic,
        Scale::MinorPentatonic,
        Scale::Blues,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Scale::Major => "Major (Ionian)",
            Scale::Minor => "Natural minor (Aeolian)",
            Scale::HarmonicMinor => "Harmonic minor",
            Scale::MelodicMinor => "Melodic minor",
            Scale::Dorian => "Dorian",
            Scale::Phrygian => "Phrygian",
            Scale::Lydian => "Lydian",
            Scale::Mixolydian => "Mixolydian",
            Scale::Locrian => "Locrian",
            Scale::MajorPentatonic => "Major pentatonic",
            Scale::MinorPentatonic => "Minor pentatonic",
            Scale::Blues => "Blues",
        }
    }

    pub fn steps(self) -> &'static [i32] {
        match self {
            Scale::Major => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Scale::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Scale::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            Scale::MajorPentatonic => &[0, 2, 4, 7, 9],
            Scale::MinorPentatonic => &[0, 3, 5, 7, 10],
            Scale::Blues => &[0, 3, 5, 6, 7, 10],
        }
    }
}

/// A highlighted key: root (0 = C .. 11 = B) and scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyScale {
    pub root: i32,
    pub scale: Scale,
}

impl KeyScale {
    /// Whether `key` (60 = C5) belongs to the scale.
    pub fn contains(self, key: i32) -> bool {
        self.scale.steps().contains(&(key - self.root).rem_euclid(12))
    }

    /// Whether `key` is the root.
    pub fn is_root(self, key: i32) -> bool {
        (key - self.root).rem_euclid(12) == 0
    }

    pub fn name(self) -> String {
        format!("{} {}", ROOTS[self.root.rem_euclid(12) as usize], self.scale.label())
    }
}

pub const ROOTS: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Chords the Draw tool can stamp, as semitones above the clicked key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chord {
    Major,
    Minor,
    Diminished,
    Augmented,
    Sus2,
    Sus4,
    Major7,
    Minor7,
    Dominant7,
    HalfDiminished7,
    Diminished7,
    Add9,
    Major9,
    Minor9,
    Power,
    Octave,
}

impl Chord {
    pub const ALL: [Chord; 16] = [
        Chord::Major,
        Chord::Minor,
        Chord::Diminished,
        Chord::Augmented,
        Chord::Sus2,
        Chord::Sus4,
        Chord::Major7,
        Chord::Minor7,
        Chord::Dominant7,
        Chord::HalfDiminished7,
        Chord::Diminished7,
        Chord::Add9,
        Chord::Major9,
        Chord::Minor9,
        Chord::Power,
        Chord::Octave,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Chord::Major => "Major",
            Chord::Minor => "Minor",
            Chord::Diminished => "Diminished",
            Chord::Augmented => "Augmented",
            Chord::Sus2 => "Sus2",
            Chord::Sus4 => "Sus4",
            Chord::Major7 => "Major 7th",
            Chord::Minor7 => "Minor 7th",
            Chord::Dominant7 => "Dominant 7th",
            Chord::HalfDiminished7 => "Minor 7th flat 5",
            Chord::Diminished7 => "Diminished 7th",
            Chord::Add9 => "Add 9",
            Chord::Major9 => "Major 9th",
            Chord::Minor9 => "Minor 9th",
            Chord::Power => "Power (5th)",
            Chord::Octave => "Octave",
        }
    }

    pub fn intervals(self) -> &'static [i32] {
        match self {
            Chord::Major => &[0, 4, 7],
            Chord::Minor => &[0, 3, 7],
            Chord::Diminished => &[0, 3, 6],
            Chord::Augmented => &[0, 4, 8],
            Chord::Sus2 => &[0, 2, 7],
            Chord::Sus4 => &[0, 5, 7],
            Chord::Major7 => &[0, 4, 7, 11],
            Chord::Minor7 => &[0, 3, 7, 10],
            Chord::Dominant7 => &[0, 4, 7, 10],
            Chord::HalfDiminished7 => &[0, 3, 6, 10],
            Chord::Diminished7 => &[0, 3, 6, 9],
            Chord::Add9 => &[0, 4, 7, 14],
            Chord::Major9 => &[0, 4, 7, 11, 14],
            Chord::Minor9 => &[0, 3, 7, 10, 14],
            Chord::Power => &[0, 7],
            Chord::Octave => &[0, 12],
        }
    }

    /// The chord's keys on `root`, those inside the key range (0..=`top`).
    pub fn keys(self, root: i32, top: i32) -> Vec<i32> {
        self.intervals().iter().map(|i| root + i).filter(|k| (0..=top).contains(k)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_hold_their_keys() {
        let c_major = KeyScale { root: 0, scale: Scale::Major };
        let white: Vec<i32> = (60..72).filter(|&k| c_major.contains(k)).collect();
        assert_eq!(white, vec![60, 62, 64, 65, 67, 69, 71]);
        let a_minor = KeyScale { root: 9, scale: Scale::Minor };
        assert!((0..132).all(|k| a_minor.contains(k) == c_major.contains(k)), "relative minor");
        assert!(a_minor.is_root(57) && !a_minor.is_root(60));
        assert_eq!(KeyScale { root: 2, scale: Scale::Dorian }.name(), "D Dorian");
        for s in Scale::ALL {
            assert_eq!(s.steps()[0], 0, "{s:?} starts on the root");
            assert!(s.steps().windows(2).all(|w| w[0] < w[1] && w[1] < 12));
        }
    }

    #[test]
    fn chords_build_on_the_root() {
        assert_eq!(Chord::Major.keys(60, 131), vec![60, 64, 67]);
        assert_eq!(Chord::Minor7.keys(57, 131), vec![57, 60, 64, 67]);
        assert_eq!(Chord::Octave.keys(125, 131), vec![125], "keys past the top are left out");
    }
}
