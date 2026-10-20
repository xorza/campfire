use std::sync::Arc;

use campfire_common::DeclaredName;
use campfire_declarations::{BookError, Listed};
use campfire_math::Num;
use campfire_script::{ApiBuilder, CallError, Frame, View};
use campfire_sim::Capability;
use campfire_units::{CollisionData, SpawnAt};
use campfire_values::{Named, Number, UnitType};
use serde::Deserialize;

use crate::action_target::ActionTarget;
use crate::actions_effect::ActionsEffect;
use crate::amount::Amount;
use crate::listed_effect::EffectResolveCx;
use crate::listed_effect::EffectRuleCx;
use crate::listed_effect::effect_data::LaunchLists;
use crate::listed_effect::error::{EffectError, EffectRule};
use crate::listed_effect::listed_spec::EffectTo;
use crate::listed_effect::listed_spec::{ListedAt, ListedSpec};
use crate::listed_effect::{ListedEffect, ListedEffects};

/// `spawn = { unit_type, duration_ms }`: a unit of the type `unit_type` of the action's package,
/// of no box body, where the effect applies, or at the point the list reached, despawning
/// `duration_ms` after it spawns when that is given.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnEffectData {
    pub unit_type: DeclaredName,
    pub duration_ms: Option<Number>,
}

/// A listed spawn, its type resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SpawnEffectSpec {
    unit_type: UnitType,
    duration_ms: Option<Amount>,
}

impl Listed for SpawnEffectData {
    type Set = ListedEffects;

    const KEY: &'static str = "spawn";

    fn into_listed(self) -> Arc<dyn ListedEffect> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}

impl ListedEffect for SpawnEffectData {
    fn needs(&self) -> Option<Capability> {
        None
    }

    fn numbers(&self) -> Vec<&Number> {
        self.duration_ms.iter().collect()
    }

    fn duration(&self) -> Option<&Number> {
        self.duration_ms.as_ref()
    }

    fn modifier(&self) -> Option<&DeclaredName> {
        None
    }

    fn lists(&self) -> Option<&LaunchLists> {
        None
    }

    fn takes_point(&self) -> bool {
        true
    }

    fn rule(&self, cx: &dyn EffectRuleCx) -> Result<(), BookError> {
        let spawned = cx.package_type(&self.unit_type);
        let Some(spawned) = spawned.filter(|unit| !unit.delivers()) else {
            return Err(cx.unknown(UnitType::NAMES, &self.unit_type));
        };
        let collision = spawned.section::<CollisionData>();
        if collision.is_some_and(|collision| collision.form.is_box()) {
            return Err(cx.fail(EffectRule::SpawnBox));
        }
        Ok(())
    }

    fn resolve(&self, cx: &mut dyn EffectResolveCx) -> Result<Arc<dyn ListedSpec>, EffectError> {
        Ok(Arc::new(SpawnEffectSpec {
            unit_type: cx.names().package_type(&self.unit_type)?,
            duration_ms: cx.amount_if(self.duration_ms.as_ref())?,
        }))
    }
}

impl ListedSpec for SpawnEffectSpec {
    /// Queues the unit where the acting unit stands, for a spawn to the source, or at the point
    /// or the unit the list reached, of the acting unit's team and player.
    fn queue(&self, at: ListedAt, frame: &mut Frame, view: &View) -> Result<(), CallError> {
        let source = frame.acting().and_then(|id| view.row(id));
        let source = source.expect("an action's list runs for its acting unit");
        let position = match (at.to, at.reached) {
            (EffectTo::Source, _) => source.position,
            (EffectTo::Reached, ActionTarget::Point(point)) => point,
            (EffectTo::Reached, ActionTarget::Unit(id)) => {
                view.row(id)
                    .expect("a list reaches a unit the view holds")
                    .position
            }
            (EffectTo::Reached, ActionTarget::None) => {
                unreachable!("the load lets a spawn reach no place only from the source")
            }
        };
        let spawn = SpawnAt {
            id: frame.take_id(),
            unit_type: self.unit_type,
            team: source.team,
            owner: source.owner,
            position,
            angle: Num::ZERO,
        };
        let life = self.duration_ms.map(|ms| ms.duration(frame, view.rate()));
        frame.queue(ActionsEffect { at: spawn, life });
        Ok(())
    }
}
