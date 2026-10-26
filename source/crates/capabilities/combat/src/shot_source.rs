use campfire_actions::{ActionSlots, ShotLaunch};
use campfire_math::Vec3;
use campfire_units::Facing;
use campfire_values::ActionId;

use crate::barrels::Barrels;
use crate::turret::Turret;
use crate::turret::turret_aim::TurretAim;

/// A unit as its shot leaves a barrel: its type's turret, if it has one, its slots, its turret's
/// aim, the way it faces, and its place, a point in space.
#[derive(Debug, Clone, Copy)]
pub struct ShotSource<'a> {
    pub turret: Option<&'a Turret>,
    pub slots: &'a ActionSlots,
    pub aim: Option<TurretAim>,
    pub facing: Option<Facing>,
    pub point: Vec3,
}

impl ShotSource<'_> {
    /// The launch of its shot of `action` from the barrel of the weapon's `barrels` that `turn`
    /// gives of their count: carried by its turret, as the turret stands, when the turret
    /// carries the slot of the weapon, then turned as it faces, at its place
    /// (04-capabilities/combat.md, A shot leaves its weapon's barrel); none for a weapon of no
    /// barrels, which `turn` is not called for.
    pub fn launch(
        &self,
        barrels: &Barrels,
        action: ActionId,
        turn: impl FnOnce(usize) -> usize,
    ) -> Option<ShotLaunch> {
        let of_weapon = barrels.of(action);
        if of_weapon.is_empty() {
            return None;
        }
        let barrel = of_weapon[turn(of_weapon.len())];
        let slot = self
            .slots
            .iter()
            .position(|slot| slot.action == Some(action));
        let slot = u8::try_from(slot.expect("a weapon that fired is in a slot"))
            .expect("a unit's slots fit u8");
        let turret = self
            .turret
            .filter(|turret| turret.carries(self.slots, slot));
        let barrel = match turret.zip(self.aim) {
            Some((turret, aim)) => turret.carried(aim, barrel),
            None => barrel,
        };
        Some(barrel.launched(self.point, self.facing))
    }
}
