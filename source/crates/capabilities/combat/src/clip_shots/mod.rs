use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declarations::ById;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::ActionId;
use campfire_visibility::DataKind;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::weapon::ClipRule;
use crate::weapon::Weapon;

/// The shots each weapon of a unit with a clip fired since its clip was last full, by action, in
/// the order of their ids: a weapon of none is not in it, so one state has one form, which its
/// owner's client predicts as it predicts its slots' times. Every unit with slots holds it, as
/// Bevy's required component, empty as the slots come. A weapon in two slots counts once, as the
/// unit attacks with its first weapon whose filter selects the target, so the second never
/// fires.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ClipShots(Vec<Fired>);

/// A weapon's shots since its clip was last full, at least one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Fired {
    action: ActionId,
    shots: u16,
}

impl ClipShots {
    /// Whether the clip of `action` is full: it fired no shot since its clip was last full.
    pub(crate) fn is_full(&self, action: ActionId) -> bool {
        self.0
            .binary_search_by_key(&action, |fired| fired.action)
            .is_err()
    }

    /// Counts a shot of `action`, of `clip` at its rank: whether the shot emptied the clip, which
    /// is then full again, so the weapon reloads.
    pub(crate) fn shoot(&mut self, action: ActionId, clip: ClipRule) -> bool {
        let at = self.0.binary_search_by_key(&action, |fired| fired.action);
        let shots = at.map_or(0, |at| self.0[at].shots) + 1;
        let emptied = shots >= clip.shots.get();
        match (at, emptied) {
            (Ok(at), true) => {
                self.0.remove(at);
            }
            (Ok(at), false) => self.0[at].shots = shots,
            (Err(_), true) => {}
            (Err(at), false) => self.0.insert(at, Fired { action, shots }),
        }
        emptied
    }
}

/// A snapshot is untrusted, so weapons out of the order of their ids, one twice, or one of no shot,
/// which a full clip is not in, fail to decode.
impl<'de> Deserialize<'de> for ClipShots {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fired = Vec::<Fired>::deserialize(deserializer)?;
        if !fired.is_sorted_by(|a, b| a.action < b.action) || fired.iter().any(|f| f.shots == 0) {
            return Err(D::Error::custom(
                "weapons in order, each once, each of a shot",
            ));
        }
        Ok(Self(fired))
    }
}

impl SimComponent for ClipShots {
    const NAME: &'static str = "combat.clip_shots";
    type Host = UnitEntity;

    // A weapon the book lacks, or one whose clip holds no more shots than it fired at every rank,
    // has no clip to count.
    fn check(&self, world: &World, _: Entity) -> bool {
        let weapons = world.resource::<ById<ActionId, Weapon>>();
        self.0.iter().all(|fired| {
            let clips = weapons
                .get(fired.action)
                .and_then(|weapon| weapon.clip.as_deref());
            clips.is_some_and(|clips| clips.iter().any(|clip| fired.shots < clip.shots.get()))
        })
    }
}

impl Replication for ClipShots {
    const KIND: DataKind = DataKind::Cooldowns;
    type Sending = SentPredicted;
}

#[cfg(test)]
mod tests;
