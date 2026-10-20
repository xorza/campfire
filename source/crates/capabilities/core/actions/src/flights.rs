use campfire_declarations::{ByType, Fact};

use crate::delivery::ProjectileFlight;

/// How each projectile type flies, which the module of projectiles makes: what an action that
/// delivers one checks its aim by.
#[derive(Debug, Default)]
pub struct Flights(pub ByType<ProjectileFlight>);

impl Fact for Flights {}
