use std::sync::Arc;

use campfire_declarations::Fact;
use campfire_values::Names;

use crate::pool_id::PoolId;

/// The names of the mode's pools, by pool, which the capability that keeps the pools gives and an
/// action's cost names its pools by.
#[derive(Debug, Clone, Default)]
pub struct PoolNames(pub Arc<Names<PoolId>>);

impl Fact for PoolNames {}
