use std::num::{NonZeroU8, NonZeroU32};

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::TickRate;

use crate::actions::action_slots::ActionSlots;
use crate::actions::slot_kind::SlotKind;
use crate::combat::combat_data::CombatData;
use crate::combat::on_death::OnDeath;
use crate::mode::error::UnitKitError;
use crate::mode::slot_action::SlotAction;
use crate::mode::unit_kit::kit_part::{KitPart, KitSpawn};
use crate::navigation::Navigation;
use crate::production::production_data::ProductionData;
use crate::progression::track_book::TrackBook;
use crate::progression::track_set::TrackSet;
use crate::stats::applier::Applier;
use crate::stats::level::Level;
use crate::stats::life::Life;
use crate::stats::lifetime::Hold;
use crate::stats::meter::Meter;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::stats::stat_book::StatBook;
use crate::stats::stats_effect::StatsEffect;
use crate::stats::unit_stats::UnitStats;
use crate::units::body::Body;
use crate::units::body_form::BodyForm;
use crate::units::modifier_id::ModifierId;
use crate::units::move_step::MoveStep;
use crate::units::owner::Owner;
use crate::units::spawn_at::SpawnAt;
use crate::units::unit_type::UnitType;
use crate::values::rank::Rank;
use crate::values::stat::{EngineStat, Stat};
use crate::vision::sight::Sight;
use crate::vision::vision_data::VisionData;

pub(crate) mod kit_part;

/// What a new unit of a type starts with, in ticks at the match's rate: its life and its other
/// pools, full at their maxima at level 1, whether it stays when it dies, when it has a `combat`
/// section, how far it walks a tick, how far it sees, its body, the tracks it gains experience on, the most trains
/// its queue holds, when it has a `production` section, whether its slots hold a build, what it
/// holds as a node, whether its slots hold a gather, and its inventory's slots and slot kind,
/// when it has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitKit {
    pub life: Option<Life>,
    pub pools: Option<Pools>,
    pub on_death: Option<OnDeath>,
    pub step: Option<MoveStep>,
    pub sight: Option<Sight>,
    pub body: Option<BodyForm>,
    pub tracks: TrackSet,
    pub queue: Option<NonZeroU8>,
    pub builds: bool,
    /// What it holds as a node, when it is one.
    pub node: Option<NonZeroU32>,
    pub gathers: bool,
    pub inventory: Option<InventorySpec>,
}

/// An inventory as a unit type gives it: how many slots, and the slot kind their actions fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventorySpec {
    pub slots: NonZeroU8,
    pub kind: SlotKind,
}

/// What a unit type's file gives its kit: its `combat` section, its pools, each with the stat of
/// its maximum, its `vision` section, its body, its tracks, its `production` section, whether
/// its slots hold a build, and its inventory.
#[derive(Debug, Clone)]
pub(crate) struct KitSections<'a, P> {
    pub(crate) combat: Option<&'a CombatData>,
    pub(crate) pools: P,
    pub(crate) vision: Option<&'a VisionData>,
    pub(crate) body: Option<BodyForm>,
    pub(crate) tracks: TrackSet,
    pub(crate) production: Option<&'a ProductionData>,
    pub(crate) builds: bool,
    pub(crate) node: Option<NonZeroU32>,
    pub(crate) gathers: bool,
    pub(crate) inventory: Option<InventorySpec>,
}

impl UnitKit {
    /// The kit of `unit_type`, of the stats `book` gives it and of its `sections`. Each pool's
    /// maximum is the stat of its maximum at level 1, and the move step is the book's at level 1
    /// at `rate`, as a refresh computes them. A type has `combat` exactly when it has the life
    /// pool `life`, which a mode with no combat lacks: a unit that can die is one that combat
    /// kills.
    pub(crate) fn new<'a>(
        book: &StatBook,
        unit_type: UnitType,
        sections: KitSections<'a, impl IntoIterator<Item = (PoolId, &'a Stat)>>,
        life: Option<PoolId>,
        rate: TickRate,
    ) -> Result<UnitKit, UnitKitError> {
        let KitSections {
            combat,
            pools,
            vision,
            body,
            tracks,
            production,
            builds,
            node,
            gathers,
            inventory,
        } = sections;
        let values = book.base_values(unit_type, 1);
        let given = |stat: &Stat| {
            let id = book.named(stat).expect("the load checked the stats");
            book.gives(unit_type, id).then_some(id)
        };
        let mut maxes = Vec::new();
        let mut full_life = None;
        for (pool, max) in pools {
            let id = given(max).ok_or_else(|| UnitKitError::MissingStat(max.clone()))?;
            let value = values[id.index()];
            let meter = Meter::new(value).ok_or_else(|| UnitKitError::NotPositive(max.clone()))?;
            if Some(pool) == life {
                full_life = Some(Life::new(meter));
            } else {
                maxes.push((pool, value));
            }
        }
        let pools =
            (!maxes.is_empty()).then(|| Pools::new(maxes).expect("each maximum is positive"));
        let on_death = match (combat, full_life.is_some()) {
            (Some(combat), true) => Some(combat.on_death),
            (Some(_), false) => return Err(UnitKitError::NoLifePool),
            (None, true) => return Err(UnitKitError::NoCombat),
            (None, false) => None,
        };
        // A mode with no move speed among its stats moves no unit.
        let moves = book
            .named(&Stat::Engine(EngineStat::MoveSpeed))
            .is_some_and(|id| book.gives(unit_type, id));
        let step = moves.then(|| {
            let step = book
                .step(&values, rate)
                .expect("a type gives only a stat the mode declares");
            MoveStep::new(step).expect("the book's step is never negative")
        });
        Ok(UnitKit {
            life: full_life,
            pools,
            on_death,
            step,
            sight: vision.map(|vision| vision.sight),
            body,
            tracks,
            queue: production.map(|production| production.queue),
            builds,
            node,
            gathers,
            inventory,
        })
    }

    /// Whether a unit of the kit, of a type of `actions`, spawns with slots: actions, or an
    /// inventory.
    pub(crate) const fn has_slots(&self, actions: &[SlotAction]) -> bool {
        self.inventory.is_some() || !actions.is_empty()
    }

    /// The slots a unit of the kit, of a type of `actions`, spawns with: its actions, each at the
    /// first rank of its kind, then an empty slot of its inventory's kind for each inventory slot;
    /// none for a type with neither.
    pub(crate) fn slots(&self, actions: &[SlotAction]) -> Option<ActionSlots> {
        if !self.has_slots(actions) {
            return None;
        }
        let slots = ActionSlots::new(
            actions
                .iter()
                .map(|action| (action.action, action.kind, action.rank)),
        );
        Some(match self.inventory {
            Some(inventory) => slots.with_empty(inventory.kind, inventory.slots.get()),
            None => slots,
        })
    }

    /// Spawns `at` with the kit, owned by `owner`: its stats, its script state at its type's
    /// defaults, the slots of its type's `actions`, its `passive`, and `parts`.
    pub(crate) fn spawn(
        &self,
        world: &mut World,
        at: SpawnAt,
        owner: Option<Owner>,
        actions: &[SlotAction],
        passive: Option<ModifierId>,
        parts: impl Bundle,
    ) -> Entity {
        let SpawnAt {
            id,
            unit_type,
            pos,
            angle,
            ..
        } = at;
        let level_track = world
            .get_resource::<TrackBook>()
            .and_then(TrackBook::level_track);
        // Its stats as its first refresh derives them, of its type at level 1 with no modifiers, so
        // a reader in the stage it spawns in finds them.
        let stats = world
            .get_resource::<StatBook>()
            .map_or_else(UnitStats::default, |book| {
                UnitStats::new(book.base_values(unit_type, Level::default().get()))
            });
        let mut unit = at.core(owner).spawn(world, (stats, parts));
        let at = KitSpawn {
            kit: self,
            level_track,
            pos,
            angle,
        };
        KitPart::give_every(&at, &mut unit);
        if let Some(slots) = self.slots(actions) {
            unit.insert(slots.timed());
        }
        let boxed = unit
            .get::<Body>()
            .is_some_and(|body| body.half_edges().is_some());
        let entity = unit.id();
        if let Some(passive) = passive {
            let applier = Applier {
                source: Some(id),
                action: None,
                rank: Rank::FIRST,
                hold: Some(Hold::Passive),
            };
            let add = StatsEffect::Add {
                target: id,
                id: passive,
                duration: None,
            };
            add.apply_by(world, applier);
        }
        if boxed {
            Navigation::make_room(world, entity);
        }
        entity
    }
}

#[cfg(test)]
mod tests;
