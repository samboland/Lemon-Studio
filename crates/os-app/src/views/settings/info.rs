//! Settings > Info: the project credits (title, author, genre, comments, web link) and its counts.
// spec: ui/settings.md "Info"

use super::{g, txt, Group, Row, SAct};
use crate::app::{App, State};

/// The checks and buttons of this page that work (`SAct::Check` / `SAct::Button` labels, handled in `act`).
pub const LIVE: &[&str] = &[];

/// The Info page's groups (spec §4).
pub fn page(s: &State) -> Vec<Group> {
    use Row::*;
    let i = &s.project.info;
    let or = |v: &Option<String>| v.clone().unwrap_or_default();
    let clips: usize = s.project.arrangements.iter().map(|a| a.clips.len()).sum();
    let notes: usize = s.project.patterns.iter().map(|p| p.notes.len()).sum();
    let mut rows = vec![
        Text("Title".into(), true),
        Big(or(&i.title)),
        Text("Author".into(), true),
        Big(or(&i.author)),
        Text("Genre".into(), true),
        Big(or(&i.genre)),
        Text("Comments".into(), true),
    ];
    let comments = or(&i.comments);
    let lines: Vec<String> = comments.lines().map(String::from).collect();
    rows.push(List(vec![("", 0.0)], lines.into_iter().map(|l| vec![l]).collect(), 14));
    rows.push(Text("Web link".into(), true));
    rows.push(Big(or(&i.url)));
    rows.push(Gap(8.0));
    rows.push(txt(format!("Channels: {}, Mixer Tracks: {}", s.project.channels.len(), s.project.mixer.len())));
    rows.push(txt(format!("Arrangements: {}, PL clips: {clips}, Notes: {notes}", s.project.arrangements.len())));
    vec![g("Project credits", rows)]
}

/// This page's working controls (`LIVE`); none yet.
pub(crate) fn act(_app: &mut App, _a: &SAct) -> bool {
    false
}
