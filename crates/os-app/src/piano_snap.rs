//! The piano roll's snap (the toolbar magnet's menu; Alt held while clicking or dragging bypasses it) and its view
//! preferences (key labels, note names). Program settings, not part of the project: kept in the user folder
//! (`pianoview.txt`).
// spec: ui/piano-roll.md §2 (toolbar), §3 (grid, keyboard), §8 (Draw tool: snapping)

/// What clicks and drags in the piano roll snap to (§2 snap menu). `Line` follows the finest grid line shown at the
/// current zoom; the others are fixed fractions of a step (a 1/16 note), a beat or a bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Snap {
    Line,
    /// No snapping: one tick.
    None,
    SixthStep,
    QuarterStep,
    ThirdStep,
    HalfStep,
    /// The grid the piano roll always had (Lemon Studio choice for the default).
    #[default]
    Step,
    SixthBeat,
    ThirdBeat,
    HalfBeat,
    Beat,
    Bar,
}

impl Snap {
    /// Menu order (§2), with a separator before each group (`None` before `Step`-fractions, then beats, then the bar).
    pub const ALL: [Snap; 12] = [
        Snap::Line,
        Snap::None,
        Snap::SixthStep,
        Snap::QuarterStep,
        Snap::ThirdStep,
        Snap::HalfStep,
        Snap::Step,
        Snap::SixthBeat,
        Snap::ThirdBeat,
        Snap::HalfBeat,
        Snap::Beat,
        Snap::Bar,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Snap::Line => "Line",
            Snap::None => "(none)",
            Snap::SixthStep => "1/6 step",
            Snap::QuarterStep => "1/4 step",
            Snap::ThirdStep => "1/3 step",
            Snap::HalfStep => "1/2 step",
            Snap::Step => "Step",
            Snap::SixthBeat => "1/6 beat",
            Snap::ThirdBeat => "1/3 beat",
            Snap::HalfBeat => "1/2 beat",
            Snap::Beat => "Beat",
            Snap::Bar => "Bar",
        }
    }

    /// The key in `pianoview.txt`.
    pub fn key(self) -> &'static str {
        match self {
            Snap::Line => "line",
            Snap::None => "none",
            Snap::SixthStep => "step/6",
            Snap::QuarterStep => "step/4",
            Snap::ThirdStep => "step/3",
            Snap::HalfStep => "step/2",
            Snap::Step => "step",
            Snap::SixthBeat => "beat/6",
            Snap::ThirdBeat => "beat/3",
            Snap::HalfBeat => "beat/2",
            Snap::Beat => "beat",
            Snap::Bar => "bar",
        }
    }

    pub fn from_key(k: &str) -> Option<Snap> {
        Snap::ALL.into_iter().find(|s| s.key() == k)
    }

    /// The snap unit in ticks (at least 1) at `ppq` with the grid zoomed to `px_per_beat`. `Line` snaps to the finest
    /// line the grid draws: steps from 5 px apart, else beats from 4 px, else bars (the same rule as the grid lines).
    pub fn ticks(self, ppq: u32, px_per_beat: f32) -> u32 {
        let step = ppq / 4;
        let t = match self {
            Snap::Line => {
                let ppt = px_per_beat / ppq.max(1) as f32;
                if step as f32 * ppt >= 5.0 {
                    step
                } else if px_per_beat >= 4.0 {
                    ppq
                } else {
                    ppq * 4
                }
            }
            Snap::None => 1,
            Snap::SixthStep => step / 6,
            Snap::QuarterStep => step / 4,
            Snap::ThirdStep => step / 3,
            Snap::HalfStep => step / 2,
            Snap::Step => step,
            Snap::SixthBeat => ppq / 6,
            Snap::ThirdBeat => ppq / 3,
            Snap::HalfBeat => ppq / 2,
            Snap::Beat => ppq,
            Snap::Bar => ppq * 4,
        };
        t.max(1)
    }
}

/// The piano roll's per-user view settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    pub snap: Snap,
    /// Name every key of the keyboard, not only the Cs (§3; Lemon Studio choice: off, as FL shows).
    pub all_labels: bool,
    /// Key names on the notes (§5; on, as before).
    pub note_names: bool,
}

impl Default for View {
    fn default() -> Self {
        View { snap: Snap::default(), all_labels: false, note_names: true }
    }
}

/// `key=value` lines: `snap` (`Snap::key`), `labels` (`all` / `c`), `names` (0 / 1); anything else is ignored.
pub fn parse(text: &str) -> View {
    let mut v = View::default();
    for line in text.lines() {
        let Some((k, val)) = line.split_once('=') else { continue };
        match (k.trim(), val.trim()) {
            ("snap", val) => v.snap = Snap::from_key(val).unwrap_or(v.snap),
            ("labels", val) => v.all_labels = val == "all",
            ("names", val) => v.note_names = val != "0",
            _ => {}
        }
    }
    v
}

pub fn text(v: View) -> String {
    format!("snap={}\nlabels={}\nnames={}\n", v.snap.key(), if v.all_labels { "all" } else { "c" }, u8::from(v.note_names))
}

/// The saved settings (tests: the defaults, so that tests never depend on the user's file).
pub fn load() -> View {
    #[cfg(test)]
    return View::default();
    #[cfg(not(test))]
    os_paths::user_file("pianoview.txt").and_then(|f| std::fs::read_to_string(f).ok()).map(|t| parse(&t)).unwrap_or_default()
}

/// Keeps the settings for the next run (tests: nothing is written).
pub fn save(v: View) {
    #[cfg(not(test))]
    if let Some(f) = os_paths::user_file("pianoview.txt") {
        let _ = std::fs::write(f, text(v));
    }
    #[cfg(test)]
    let _ = text(v);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_units_at_ppq_96() {
        let t = |s: Snap| s.ticks(96, 96.0);
        assert_eq!(t(Snap::Step), 24);
        assert_eq!(t(Snap::None), 1);
        assert_eq!((t(Snap::SixthStep), t(Snap::QuarterStep), t(Snap::ThirdStep), t(Snap::HalfStep)), (4, 6, 8, 12));
        assert_eq!((t(Snap::SixthBeat), t(Snap::ThirdBeat), t(Snap::HalfBeat), t(Snap::Beat), t(Snap::Bar)), (16, 32, 48, 96, 384));
        // Line follows the zoom: steps while they are 5 px apart, then beats, then bars.
        assert_eq!(Snap::Line.ticks(96, 96.0), 24);
        assert_eq!(Snap::Line.ticks(96, 12.0), 96);
        assert_eq!(Snap::Line.ticks(96, 3.0), 384);
        // A coarse PPQ never gives 0.
        assert_eq!(Snap::SixthStep.ticks(4, 96.0), 1);
    }

    #[test]
    fn settings_file_round_trips() {
        assert_eq!(parse(""), View::default());
        let v = View { snap: Snap::HalfBeat, all_labels: true, note_names: false };
        assert_eq!(parse(&text(v)), v);
        assert_eq!(parse("snap=nope\nlabels=c\njunk"), View::default());
        for s in Snap::ALL {
            assert_eq!(Snap::from_key(s.key()), Some(s));
        }
    }
}
