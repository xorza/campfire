use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_actions::{ActionSlots, Shots};
use campfire_declarations::ById;
use campfire_sim::{EntityIndex, SimComponent};
use campfire_states::{NotSent, Replication};
use campfire_units::{Facing, MapBook, UnitEntity};
use campfire_values::{ActionId, UnitType};
use campfire_visibility::DataKind;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::barrels::Barrels;
use crate::shot_source::ShotSource;
use crate::turret::Turret;
use crate::turret::turret_aim::TurretAim;

/// The barrel each weapon of a unit launches its next shot from, by action, in the order of their
/// ids, of the weapons whose next barrel is not their first: a weapon at its first is not in it,
/// so one state has one form (04-capabilities/combat.md, A shot leaves its weapon's barrel). Every
/// unit with slots holds it on a server, as Bevy's required component, empty as the slots come;
/// a client never holds it, as the server alone launches.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct BarrelTurns(Vec<Turned>);

/// A weapon's next barrel past its first, by its place among the weapon's barrels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Turned {
    action: ActionId,
    barrel: u16,
}

impl BarrelTurns {
    /// The place among `count` barrels of the barrel the shot of `action` leaves from, which
    /// then turns to the next, back to the first past the last, as `Weapon::fireWeapon` turns
    /// its barrel with each shot.
    pub(crate) fn turn(&mut self, action: ActionId, count: usize) -> usize {
        debug_assert!(count > 0, "a weapon of barrels has one");
        let at = self.0.binary_search_by_key(&action, |turned| turned.action);
        let barrel = at.map_or(0, |at| usize::from(self.0[at].barrel)) % count;
        let next = u16::try_from((barrel + 1) % count).expect("a weapon's barrels fit a u16");
        match (at, next) {
            (Ok(at), 0) => {
                self.0.remove(at);
            }
            (Ok(at), next) => self.0[at].barrel = next,
            (Err(_), 0) => {}
            (Err(at), barrel) => self.0.insert(at, Turned { action, barrel }),
        }
        barrel
    }

    /// Gives each of the tick's shots of a weapon of barrels its launch from the barrel its
    /// weapon fires next, in the order fired, as `ShotSource::launch` gives it. A shot whose source
    /// is gone keeps none. The server alone runs it, as it alone launches.
    pub(crate) fn launch(
        (barrels, turrets, map): (
            Res<'_, ById<UnitType, Barrels>>,
            Res<'_, ById<UnitType, Turret>>,
            Res<'_, MapBook>,
        ),
        index: Res<'_, EntityIndex>,
        mut shots: ResMut<'_, Shots>,
        mut units: Query<
            '_,
            '_,
            (
                &UnitType,
                &ActionSlots,
                Option<&Facing>,
                Option<&TurretAim>,
                &mut Self,
            ),
        >,
    ) {
        for shot in shots.iter_mut() {
            let unit = index
                .get(shot.source)
                .and_then(|unit| units.get_mut(unit).ok());
            let Some((&unit_type, slots, facing, aim, mut turns)) = unit else {
                continue;
            };
            let Some(barrels) = barrels.get(unit_type) else {
                continue;
            };
            let source = ShotSource {
                turret: turrets.get(unit_type),
                slots,
                aim: aim.copied(),
                facing: facing.copied(),
                point: map.point(shot.from),
            };
            let action = shot.damage.action;
            if let Some(launch) = source.launch(barrels, action, |count| turns.turn(action, count))
            {
                shot.launch = Some(launch);
            }
        }
    }
}

/// A snapshot is untrusted, so weapons out of the order of their ids, one twice, or one at its
/// first barrel, which is not in it, fail to decode.
impl<'de> Deserialize<'de> for BarrelTurns {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let turned = Vec::<Turned>::deserialize(deserializer)?;
        let ordered = turned.is_sorted_by(|a, b| a.action < b.action);
        if !ordered || turned.iter().any(|turned| turned.barrel == 0) {
            return Err(D::Error::custom(
                "weapons in order, each once, each past its first barrel",
            ));
        }
        Ok(Self(turned))
    }
}

impl SimComponent for BarrelTurns {
    const NAME: &'static str = "combat.barrel_turns";
    type Host = UnitEntity;

    // Each weapon it turns has more barrels on its unit's type than its next barrel's place.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let barrels = world
            .get::<UnitType>(entity)
            .and_then(|&unit_type| world.resource::<ById<UnitType, Barrels>>().get(unit_type));
        self.0.iter().all(|turned| {
            barrels
                .is_some_and(|barrels| usize::from(turned.barrel) < barrels.of(turned.action).len())
        })
    }
}

impl Replication for BarrelTurns {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}

#[cfg(test)]
mod tests;
