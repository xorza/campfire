use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, Allow, Changed, Or, With, Without};
use bevy_ecs::system::{Local, ParamSet, Query, Res, ResMut};
use campfire_math::Num;
use campfire_sim::{EntityIndex, StableId, TickRate, Unpredicted};
use campfire_units::{Dead, MoveStep, StatusTags, TagEffect};
use campfire_values::{UnitTags, UnitType};

use crate::level::Level;
use crate::life::Life;
use crate::live_carriers::LiveCarriers;
use crate::modifiers::Modifiers;
use crate::param_book::ParamBook;
use crate::param_source::ParamSource;
use crate::pool_book::PoolBook;
use crate::pool_id::PoolId;
use crate::pools::Pools;
use crate::refresh_scratch::{RefreshScratch, Refreshing};
use crate::stat_book::StatBook;
use crate::stat_books::StatBooks;
use crate::unit_pools::UnitPools;
use crate::unit_stats::UnitStats;

/// The derived parts of units: their stats from their modifiers and level, and their pools as
/// they regenerate.
#[derive(Debug)]
pub struct Refresh;

impl Refresh {
    /// Derives the stats and tags of every unit whose level, modifiers or type changed, as a
    /// client may receive its type after its level, that is new, or
    /// that carries a live change of a unit it refreshes, a chain of them whole, and sets what holds their effect: how far it walks a tick and
    /// its pools' maxima, a pool keeping the rule of stats.md. A modifier its tags' immunities
    /// suppress gives no tags and no stats. Each stat is computed for every refreshing unit in the
    /// stat book's order, so a live change reads its source's stats once they are final; a live
    /// change whose source is gone keeps the value it last had. An effect whose stat the mode does
    /// not declare keeps what the unit's type gave it as it spawned. A predicting client derives
    /// the stats and tags of every unit it holds, as their inputs reach it, so an aura's numbers
    /// resolve there as on the server (design 14, D11); it writes nothing else of a unit it holds
    /// as the server sent it.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    pub(crate) fn run(
        books: StatBooks<'_>,
        index: Res<'_, EntityIndex>,
        params: Res<'_, ParamBook>,
        rate: Res<'_, TickRate>,
        mut carriers: ResMut<'_, LiveCarriers>,
        mut units: ParamSet<
            '_,
            '_,
            (
                Query<
                    '_,
                    '_,
                    Entity,
                    (
                        With<UnitStats>,
                        Or<(
                            Changed<Level>,
                            Changed<Modifiers>,
                            Changed<StatusTags>,
                            Changed<UnitType>,
                            Added<UnitStats>,
                        )>,
                        Allow<Unpredicted>,
                    ),
                >,
                Query<
                    '_,
                    '_,
                    (
                        &StableId,
                        &UnitType,
                        &Level,
                        Option<&Modifiers>,
                        Option<&mut UnitTags>,
                        Option<&StatusTags>,
                    ),
                    (With<UnitStats>, Allow<Unpredicted>),
                >,
                Query<'_, '_, (&UnitType, &Level, &UnitStats), Allow<Unpredicted>>,
                Query<'_, '_, &mut UnitStats, Allow<Unpredicted>>,
                Query<'_, '_, (Option<&mut MoveStep>, UnitPools)>,
                Query<'_, '_, &mut Modifiers>,
            ),
        >,
        mut scratch: Local<'_, RefreshScratch>,
        mut visits: Local<'_, Vec<Entity>>,
    ) {
        let StatBooks {
            stats: book,
            pools: pool_book,
            modifiers: modifier_book,
            tags: tag_book,
        } = books;
        let scratch = &mut *scratch;
        scratch.clear();
        visits.clear();
        visits.extend(units.p0().iter());
        let ids = units.p1();
        let id_of = |entity| ids.get(entity).ok().map(|(&id, ..)| id);
        carriers.join_sources(&mut visits, id_of);
        carriers.forget(
            &visits,
            |id| index.get(id).is_some(),
            |entity| ids.contains(entity),
        );
        let granting = tag_book.granting();
        let mut refreshing = units.p1();
        for &entity in &*visits {
            let Ok((&id, &unit_type, level, modifiers, tags, status)) = refreshing.get_mut(entity)
            else {
                continue;
            };
            let stacked = modifiers.into_iter().flat_map(Modifiers::stacked);
            let granted = modifier_book.granted(stacked, status.copied());
            let derived = tag_book.unit_tags(unit_type, granted);
            if let Some(mut tags) = tags {
                tags.set_if_neq(derived);
            }
            let immune = derived.immune;
            let takes_effect = TagEffect::new(granting, immune);
            let unit = Refreshing {
                id,
                entity,
                unit_type,
                level: level.get(),
            };
            scratch.add(&book, &modifier_book, unit, modifiers, |tags| {
                takes_effect.takes(tags)
            });
        }
        if scratch.units.is_empty() {
            return;
        }
        let sources = units.p2();
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
        let life = pool_book.life();
        let mut stats = units.p3();
        for (unit, refreshing) in scratch.units.iter().enumerate() {
            let Ok(mut stats) = stats.get_mut(refreshing.entity) else {
                continue;
            };
            let values = scratch.values(unit, count);
            // A unit with a live change refreshes every pass, mostly to the values it has.
            if stats.values() != values {
                stats.refill().extend_from_slice(values);
            }
        }
        Refresh::hold_effects(scratch, (&book, &pool_book), life, *rate, &mut units.p4());
        // The value a live change last had is state, so a replay keeps it; its write marks a
        // change, which refreshes its carrier once more, to the value it now holds.
        let mut holders = units.p5();
        for term in scratch.lives() {
            let entity = scratch.units[term.unit as usize].entity;
            if let Some(source) = term.source() {
                carriers.add(source, entity);
            }
            if let Ok(mut modifiers) = holders.get_mut(entity) {
                let at = term.share as usize;
                if modifiers.share_value(at) != term.value {
                    modifiers.set_share_value(at, term.value);
                }
            }
        }
        carriers.sort();
    }

    /// Sets what holds the effect of each refreshed unit's stats, `scratch`'s values: how far it
    /// walks a tick at `rate`, and its pools' maxima, `life` the life pool's. A unit a client
    /// holds as the server sent it is no unit of `effects`, so a client writes nothing of it.
    fn hold_effects(
        scratch: &RefreshScratch,
        (book, pool_book): (&StatBook, &PoolBook),
        life: Option<PoolId>,
        rate: TickRate,
        effects: &mut Query<'_, '_, (Option<&mut MoveStep>, UnitPools)>,
    ) {
        let count = usize::from(book.len());
        for (unit, refreshing) in scratch.units.iter().enumerate() {
            let Ok((step, pools)) = effects.get_mut(refreshing.entity) else {
                continue;
            };
            let values = scratch.values(unit, count);
            if let (Some(mut step), Some(value)) = (step, book.step(values, rate)) {
                step.set_if_neq(MoveStep::new(value).expect("a step is at least 0"));
            }
            let mut pools = pools.of(life);
            for (pool, stats) in pool_book.iter() {
                let max = values[stats.max.index()].max(Num::EPSILON);
                pools.set_max(pool, max);
            }
        }
    }

    /// Adds each living unit's regen to its pools: each pool's `regen` stat a second, the tick
    /// rate's share a tick, the remainder carried so a second gains exactly the regen.
    pub(crate) fn regenerate(
        (pool_book, rate): (Res<'_, PoolBook>, Res<'_, TickRate>),
        mut units: Query<
            '_,
            '_,
            (&UnitStats, UnitPools),
            (Or<(With<Life>, With<Pools>)>, Without<Dead>),
        >,
    ) {
        let hz = rate.hz().get();
        let life = pool_book.life();
        for (stats, pools) in &mut units {
            let values = stats.values();
            let mut pools = pools.of(life);
            for (pool, stats) in pool_book.iter() {
                if let Some(regen) = stats.regen {
                    pools.regen(pool, values[regen.index()], hz);
                }
            }
        }
    }
}
