use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, Changed, Has, Or, With, Without};
use bevy_ecs::system::{Commands, Local, ParamSet, Query, Res};
use campfire_math::Num;
use campfire_sim::{EntityIndex, StableId, TickRate};

use crate::stats::level::Level;
use crate::stats::live_shares::LiveShares;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifiers::Modifiers;
use crate::stats::move_step::MoveStep;
use crate::stats::param_book::ParamBook;
use crate::stats::param_source::ParamSource;
use crate::stats::pool_book::PoolBook;
use crate::stats::pools::Pools;
use crate::stats::refresh_scratch::{RefreshScratch, Refreshing};
use crate::stats::stat_book::StatBook;
use crate::stats::unit_stats::UnitStats;
use crate::units::dead::Dead;
use crate::units::tag_book::TagBook;
use crate::units::tag_set::TagSet;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;

/// The derived parts of units: their stats from their modifiers and level, and their pools as
/// they regenerate.
#[derive(Debug)]
pub(crate) struct Refresh;

impl Refresh {
    /// Gives each unit of a type that lacks them the parts its stats and tags derive into, which a
    /// restore does not bring back, as they are never state; the refresh after it derives them, as
    /// it does a new unit's.
    pub(crate) fn give_parts(
        book: Option<Res<'_, StatBook>>,
        mut commands: Commands<'_, '_>,
        units: Query<'_, '_, Entity, (With<UnitType>, Without<UnitStats>)>,
    ) {
        if book.is_none() {
            return;
        }
        for unit in &units {
            commands
                .entity(unit)
                .insert((UnitStats::default(), UnitTags::default()));
        }
    }

    /// Derives the stats and tags of every unit whose level or modifiers changed, that is new, or
    /// that carries a live change, and sets what holds their effect: how far it walks a tick and
    /// its pools' maxima, a pool keeping the rule of stats.md. A modifier its tags' immunities
    /// suppress gives no tags and no stats. Each stat is computed for every refreshing unit in the
    /// stat book's order, so a live change reads its source's stats once they are final; a live
    /// change whose source is gone keeps the value it last had. An effect whose stat the mode does
    /// not declare keeps what the unit's kit gave it.
    pub(crate) fn run(
        (book, pool_book, tag_book, modifier_book, index): (
            Option<Res<'_, StatBook>>,
            Option<Res<'_, PoolBook>>,
            Option<Res<'_, TagBook>>,
            Option<Res<'_, ModifierBook>>,
            Res<'_, EntityIndex>,
        ),
        (params, rate): (Res<'_, ParamBook>, Res<'_, TickRate>),
        mut commands: Commands<'_, '_>,
        mut units: ParamSet<
            '_,
            '_,
            (
                Query<
                    '_,
                    '_,
                    (
                        Entity,
                        &StableId,
                        &UnitType,
                        &Level,
                        Option<&Modifiers>,
                        Option<&mut UnitTags>,
                        Has<LiveShares>,
                    ),
                    (
                        With<UnitStats>,
                        Or<(
                            Changed<Level>,
                            Changed<Modifiers>,
                            Added<UnitStats>,
                            With<LiveShares>,
                        )>,
                    ),
                >,
                Query<'_, '_, (&UnitType, &Level, &UnitStats)>,
                Query<'_, '_, (&mut UnitStats, Option<&mut MoveStep>, Option<&mut Pools>)>,
                Query<'_, '_, &mut Modifiers>,
            ),
        >,
        mut scratch: Local<'_, RefreshScratch>,
    ) {
        let (Some(book), Some(pool_book), Some(modifier_book)) = (book, pool_book, modifier_book)
        else {
            return;
        };
        let scratch = &mut *scratch;
        scratch.clear();
        let granting = tag_book
            .as_deref()
            .map_or(TagSet::default(), TagBook::granting);
        for (entity, &id, &unit_type, level, modifiers, tags, marked) in &mut units.p0() {
            let held = modifiers.into_iter().flat_map(Modifiers::iter);
            let granted = held
                .filter(|instance| instance.stacks > 0)
                .map(|instance| modifier_book.tags(instance.id));
            let derived = tag_book
                .as_deref()
                .map(|book| book.unit_tags(unit_type, granted));
            if let (Some(mut tags), Some(derived)) = (tags, derived) {
                tags.set_if_neq(derived);
            }
            let immune = derived.map_or(TagSet::default(), |derived| derived.immune);
            let takes_effect = TagBook::effect_test(granting, immune);
            let unit = Refreshing {
                id,
                entity,
                unit_type,
                level: level.get(),
            };
            let live = scratch.add(&book, &modifier_book, unit, modifiers, takes_effect);
            if live && !marked {
                commands.entity(entity).insert(LiveShares);
            } else if !live && marked {
                commands.entity(entity).remove::<LiveShares>();
            }
        }
        if scratch.units.is_empty() {
            return;
        }
        let sources = units.p1();
        let other = |id| {
            let (&unit_type, level, stats) = sources.get(index.get(id)?).ok()?;
            Some(ParamSource::of_parts(
                &book,
                unit_type,
                Some(level),
                Some(stats),
            ))
        };
        scratch.compute(&book, &params, other);
        let count = usize::from(book.len());
        let mut writes = units.p2();
        for (unit, refreshing) in scratch.units.iter().enumerate() {
            let Ok((mut stats, step, pools)) = writes.get_mut(refreshing.entity) else {
                continue;
            };
            let values = scratch.values(unit, count);
            let refill = stats.refill();
            refill.extend_from_slice(values);
            if let (Some(mut step), Some(value)) = (step, book.step(values, *rate)) {
                step.set_if_neq(MoveStep::new(value).expect("a step is at least 0"));
            }
            if let Some(mut pools) = pools {
                let mut changed = *pools;
                for (pool, stats) in pool_book.iter() {
                    let max = values[stats.max.index()].max(Num::EPSILON);
                    changed.set_max(pool, max);
                }
                pools.set_if_neq(changed);
            }
        }
        let mut carriers = units.p3();
        for term in scratch.lives() {
            let entity = scratch.units[term.unit as usize].entity;
            if let Ok(mut modifiers) = carriers.get_mut(entity) {
                // The value is state, so a replay keeps it, and no change of the modifiers: writing it
                // marks nothing, or the refresh would run again for it.
                let at = term.share as usize;
                modifiers
                    .bypass_change_detection()
                    .set_share_value(at, term.value);
            }
        }
    }

    /// Adds each living unit's regen to its pools: each pool's `regen` stat a second, the tick
    /// rate's share a tick, the remainder carried so a second gains exactly the regen.
    pub(crate) fn regenerate(
        (book, pool_book, rate): (
            Option<Res<'_, StatBook>>,
            Option<Res<'_, PoolBook>>,
            Res<'_, TickRate>,
        ),
        mut units: Query<'_, '_, (&UnitStats, &mut Pools), Without<Dead>>,
    ) {
        let (Some(_), Some(pool_book)) = (book, pool_book) else {
            return;
        };
        let hz = rate.hz().get();
        for (stats, mut pools) in &mut units {
            let values = stats.values();
            let mut changed = *pools;
            for (pool, stats) in pool_book.iter() {
                if let Some(regen) = stats.regen {
                    changed.regen(pool, values[regen.index()], hz);
                }
            }
            pools.set_if_neq(changed);
        }
    }
}
