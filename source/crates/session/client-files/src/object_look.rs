use campfire_common::DeclaredName;
use serde::{Deserialize, Serialize};

use crate::client_model::ClientModel;
use crate::client_unit::{DrawnLook, LookVariant};

/// The look of a map's client object: the models drawn at its place, all of them, and its
/// variants, the models it draws in their stead on a map of their conditions; no client object is
/// built, turns a turret or varies, so a file that gives one those fails to read
/// (02-engine-core.md, A client object's look is models and variants).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectLook {
    pub models: Vec<ClientModel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<LookVariant>,
}

impl ObjectLook {
    /// The look of `models`, of no variant.
    pub const fn new(models: Vec<ClientModel>) -> Self {
        Self {
            models,
            variants: Vec::new(),
        }
    }

    /// What its object draws.
    pub fn drawn(&self) -> DrawnLook<'_> {
        DrawnLook {
            models: &self.models,
            turret: None,
        }
    }

    /// Draws, on a map of `conditions`, its models as `ClientUnit::settle` picks a look's own;
    /// and holds no variant after.
    pub fn settle(&mut self, conditions: &[DeclaredName]) {
        LookVariant::settle(&mut self.models, &mut self.variants, conditions);
    }
}
