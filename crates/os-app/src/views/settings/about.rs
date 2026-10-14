//! Settings > About, which also carries what FL's Account tab had (spec: ui/plugin-manager.md §2).
// spec: ui/settings.md, ui/plugin-manager.md §2

use super::{g, txt, Group, Row};
use crate::app::State;

/// The About page's groups (spec §4).
pub fn page(_s: &State) -> Vec<Group> {
    use Row::*;
    vec![
        g("", vec![
            Gap(20.0),
            Big(format!("Lemon Studio {}", env!("CARGO_PKG_VERSION"))),
            txt("A digital audio workstation written in Rust."),
            Gap(10.0),
            txt("Licence: GNU General Public License v3."),
            txt("Fonts: Cuprum (SIL Open Font License). Icons: Tabler Icons (MIT)."),
        ]),
        g("Licence", vec![
            Big("Lemon Studio needs no account.".into()),
            txt("It is free software under the GNU General Public License, version 3."),
            txt("No registration, no online services, no usage data."),
        ]),
    ]
}
