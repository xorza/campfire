use bevy_ecs::world::World;
use campfire_actions::{ActionBook, ActionSlots, SlotTimes};
use campfire_common::Ticks;
use campfire_declarations::ById;
use campfire_math::Num;
use campfire_script::{Effect, EffectKind, Frame};
use campfire_sim::{EntityIndex, SimTick, SimWorld, StableId};
use campfire_stats::UnitStats;
use campfire_units::{Owner, UnitsWorld};
use campfire_values::{ActionId, Allegiance, DamageKind, TagSet, Team, UnitTags};

use crate::armed::Armed;
use crate::damage::{Damage, DamageCause};
use crate::damage_pass::DamagePass;
use crate::heal::{Heal, HealCause};
use crate::pass_queue::PassEntry;
use crate::respawn::Respawn;
use crate::type_armor::TypeArmor;
use crate::weapon::Weapon;

/// A change to units' pools that a call queued, from its acting unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CombatEffect {
    /// `amount` of `kind` damage to `unit` from `source`, else from the acting unit, which joins
    /// the damage pass.
    Damage {
        unit: StableId,
        amount: Num,
        kind: DamageKind,
        source: Option<StableId>,
    },
    /// A heal of `amount` to `unit`'s life pool, times one plus its `heal_scale` stat.
    Heal { unit: StableId, amount: Num },
    /// An extra attack of the acting unit on `unit`, with the weapon it would choose against
    /// `unit` (combat-response.md, Combat chooses the weapon).
    AttackHit { unit: StableId },
    /// `unit`'s life taken whole, so it dies as the next Die stage finds it, with no killer.
    Kill { unit: StableId },
    /// `unit`, dead and of a type that stays, back `ticks` after the call's tick starts.
    Respawn { unit: StableId, ticks: Ticks },
}

/// From the call's acting unit and its ability, at its depth of the chain of combat events.
impl Effect for CombatEffect {
    const KIND: EffectKind = EffectKind::Change;

    /// Applies the effect, which the call in `frame` queued: from its acting unit, by its ability,
    /// at its chain depth, delivered by its hit. Damage and a heal join the pass's queue, and an
    /// extra attack queues the damage of the weapon the source would choose against its target,
    /// when it still has one.
    fn apply(self, world: &mut World, frame: &mut Frame) {
        let (source, action, depth) = (frame.acting(), frame.action(), frame.depth());
        let damage = |target, amount, kind, cause| Damage {
            source,
            target,
            amount,
            kind,
            cause,
            action,
            depth,
            hit: frame.hit(),
        };
        match self {
            Self::Damage {
                unit,
                amount,
                kind,
                source: named,
            } => {
                let damage = Damage {
                    source: named.or(source),
                    ..damage(unit, amount, kind, DamageCause::Effect)
                };
                DamagePass::queue(world, PassEntry::Damage(damage));
            }
            Self::Heal { unit, amount } => {
                let heal = Heal {
                    source,
                    target: unit,
                    amount,
                    cause: HealCause::Effect,
                    action,
                    depth,
                };
                DamagePass::queue(world, PassEntry::Heal(heal));
            }
            Self::Kill { unit } => world.drain_life(unit),
            Self::Respawn { unit, ticks } => {
                let entity = world.entity_of(unit);
                let entity = entity.expect("a dead unit that stays is in the world");
                let at = frame.now().after(ticks);
                world.entity_mut(entity).insert(Respawn { at });
            }
            Self::AttackHit { unit: target } => {
                let index = world.resource::<EntityIndex>();
                let (Some(unit), Some(aimed)) = (
                    source.and_then(|source| index.get(source)),
                    index.get(target),
                ) else {
                    return;
                };
                let (Some(&team), Some(&aimed_team)) =
                    (world.get::<Team>(unit), world.get::<Team>(aimed))
                else {
                    return;
                };
                let tags = world
                    .get::<UnitTags>(aimed)
                    .map_or(TagSet::NONE, |tags| tags.tags);
                let selected = (world.relations().between(team, aimed_team), tags);
                let armor = TypeArmor::of_unit(world, aimed);
                let now = world.resource::<SimTick>().start();
                let unit = world.entity(unit);
                let book = world.resource::<ActionBook>();
                let weapons = world.resource::<ById<ActionId, Weapon>>();
                let stats = unit.get::<UnitStats>().map_or(&[][..], UnitStats::values);
                let chosen = unit.get::<ActionSlots>().and_then(|slots| {
                    let armed = Armed {
                        slots,
                        times: unit.get::<SlotTimes>(),
                        book,
                        weapons,
                        holder: unit
                            .get::<UnitTags>()
                            .map_or(TagSet::NONE, |tags| tags.tags),
                        stats,
                        now,
                    };
                    // An extra attack strikes at once, at any distance.
                    let slot = slots.slot(armed.choose(selected, armor, |_| true)?)?;
                    Some((slot, slot.rank?, weapons.get(slot.action?)?))
                });
                let Some((slot, rank, weapon)) = chosen else {
                    return;
                };
                let allegiance = Allegiance {
                    team,
                    owner: unit.get::<Owner>().map(|owner| owner.player_slot()),
                };
                let hit = Damage {
                    action: slot.action,
                    ..damage(
                        target,
                        weapon.damage(stats, rank),
                        weapon.kind,
                        DamageCause::ExtraAttack { rank, allegiance },
                    )
                };
                DamagePass::queue(world, PassEntry::Damage(hit));
            }
        }
    }
}
