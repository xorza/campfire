use std::sync::Arc;

use campfire_declare::Fact;
use campfire_values::Names;

use crate::stat_id::StatId;

/// The names of the stats the mode declares, by stat, which an action's param scales by: a fact
/// the capability that keeps the stats gives.
#[derive(Debug, Clone, Default)]
pub struct StatNames(pub Arc<Names<StatId>>);

impl Fact for StatNames {}
