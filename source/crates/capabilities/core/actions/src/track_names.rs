use std::sync::Arc;

use campfire_declarations::Fact;
use campfire_values::{Names, TrackId};

/// The names of the mode's tracks, by track, which the module of experience makes: what an
/// action's effect lists name a track by.
#[derive(Debug, Clone, Default)]
pub struct TrackNames(pub Arc<Names<TrackId>>);

impl Fact for TrackNames {}
