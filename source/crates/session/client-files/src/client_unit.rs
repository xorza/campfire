use campfire_common::DeclaredName;
use campfire_math::Num;
use serde::{Deserialize, Serialize};

use crate::client_model::{ClientModel, ModelTop};

/// A unit type's look: the models drawn at its unit's place, all of them; its variants, the
/// models it draws in their stead on a map of their conditions; the look of each of its type's
/// variations; and the angle in degrees a client's preview of a build places it at, 0 when it
/// gives none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientUnit {
    pub models: Vec<ClientModel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<LookVariant>,
    /// By its type's variations' order, what a unit of each draws in place of the look's own;
    /// none for a look that does not vary, whose own a unit of any variation draws, as a build
    /// preview and a client object do (02-engine-core.md, A look varies by its unit's variation).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variations: Vec<LookVariation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement_angle: Option<Num>,
    /// The name of the node its models turn by the unit's turret angle
    /// (04-capabilities/combat.md, A unit type may have a turret).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turret: Option<String>,
}

/// The look of one of a type's variations: its models, its variants, and the name of the node its
/// models turn by the unit's turret angle, as each variation's models name their own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookVariation {
    pub models: Vec<ClientModel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<LookVariant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turret: Option<String>,
}

/// What a unit of a look draws: its models and the name of the node they turn by its turret
/// angle, the look's own or one variation's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawnLook<'a> {
    pub models: &'a [ClientModel],
    pub turret: Option<&'a str>,
}

/// A look's models on a map of some conditions, as a night or snowy weather, by name: any game
/// names its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookVariant {
    pub conditions: Vec<DeclaredName>,
    pub models: Vec<ClientModel>,
}

impl ClientUnit {
    /// The look of `models`, of no variant or variation.
    pub const fn new(models: Vec<ClientModel>) -> Self {
        Self {
            models,
            variants: Vec::new(),
            variations: Vec::new(),
            placement_angle: None,
            turret: None,
        }
    }

    /// What a unit of no variation of its own draws.
    pub fn own(&self) -> DrawnLook<'_> {
        DrawnLook {
            models: &self.models,
            turret: self.turret.as_deref(),
        }
    }

    /// Draws, on a map of `conditions`, its own models and each variation's as
    /// `LookVariant::settle` picks them; and holds no variant after.
    pub fn settle(&mut self, conditions: &[DeclaredName]) {
        LookVariant::settle(&mut self.models, &mut self.variants, conditions);
        for variation in &mut self.variations {
            LookVariant::settle(&mut variation.models, &mut variation.variants, conditions);
        }
    }
}

impl LookVariation {
    /// What a unit of the variation draws.
    pub fn drawn(&self) -> DrawnLook<'_> {
        DrawnLook {
            models: &self.models,
            turret: self.turret.as_deref(),
        }
    }
}

impl DrawnLook<'_> {
    /// The top of the highest of its models, of those whose top is known; none when none is.
    pub fn top(self) -> Option<ModelTop> {
        let tops = self.models.iter().filter_map(|model| model.top);
        tops.reduce(|top, other| if other.get() > top.get() { other } else { top })
    }
}

impl LookVariant {
    /// Puts in `models`, on a map of `conditions`, the models of the variant of `variants` whose
    /// conditions have the most in common with the map's, of a tie the one of the fewest the map
    /// lacks, of a tie the first, `models` counted first as of no condition; and empties
    /// `variants`.
    pub(crate) fn settle(
        models: &mut Vec<ClientModel>,
        variants: &mut Vec<Self>,
        conditions: &[DeclaredName],
    ) {
        let mut best = None;
        let (mut most, mut fewest) = (0, 0);
        for (at, variant) in variants.iter().enumerate() {
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
        let mut variants = std::mem::take(variants);
        if let Some(at) = best {
            *models = variants.swap_remove(at).models;
        }
    }
}
