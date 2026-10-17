use std::sync::Arc;

use campfire_declare::Fact;
use campfire_values::{ModifierId, Names};

/// The names of the modifiers the packages declare, each by its package and its name, which an
/// action's passive, hold or effect names: a fact the capability that keeps the modifiers gives.
#[derive(Debug, Clone, Default)]
pub struct ModifierNames(pub Arc<Names<ModifierId>>);

impl Fact for ModifierNames {}
