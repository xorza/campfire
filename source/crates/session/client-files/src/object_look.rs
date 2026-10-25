use campfire_common::DeclaredName;
use serde::{Deserialize, Serialize};

use crate::client_model::ClientModel;

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

/// An object look's models on a map of some conditions, as a night or snowy weather, by name:
/// any game names its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookVariant {
    pub conditions: Vec<DeclaredName>,
    pub models: Vec<ClientModel>,
}

impl ObjectLook {
    /// The look of `models`, of no variant.
    pub const fn new(models: Vec<ClientModel>) -> Self {
        Self {
            models,
            variants: Vec::new(),
        }
    }

    /// Puts in its models, on a map of `conditions`, the models of its variant whose conditions
    /// have the most in common with the map's, of a tie the one of the fewest the map lacks, of a
    /// tie the first, its own models counted first as of no condition; and holds no variant
    /// after.
    pub fn settle(&mut self, conditions: &[DeclaredName]) {
        let mut best = None;
        let (mut most, mut fewest) = (0, 0);
        for (at, variant) in self.variants.iter().enumerate() {
            let common = variant
                .conditions
                .iter()
                .filter(|condition| conditions.contains(condition))
                .count();
            let lacked = variant.conditions.len() - common;
            if common > most || (common >= most && lacked < fewest) {
                (best, most, fewest) = (Some(at), common, lacked);
            }
        }
        let mut variants = std::mem::take(&mut self.variants);
        if let Some(at) = best {
            self.models = variants.swap_remove(at).models;
        }
    }
}
