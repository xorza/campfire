use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use serde::Deserialize;

use crate::track_data::TrackData;

/// The mode's `[tracks]`: the tracks its units gain experience on, by name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct ModeTracksData(pub BTreeMap<DeclaredName, TrackData>);

impl Section for ModeTracksData {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "tracks";
}
