use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_sim::{Position, SimTick, StableId, TickRate};

use crate::geometry::metric::Metric;
use crate::geometry::shape::Shape;
use crate::stats::applier::Applier;
use crate::stats::carried_mut::CarriedMut;
use crate::stats::held_modifiers::{Held, HeldModifiers};
use crate::stats::lifetime::Hold;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifier_spec::ParamPlace;
use crate::stats::modifiers::Modifiers;
use crate::stats::param_book::ParamBook;
use crate::stats::param_sources::ParamSources;
use crate::stats::player_modifiers::PlayerModifiers;
use crate::stats::stat_book::StatBook;
use crate::units::body::Body;
use crate::units::body_grid::{BodyGrid, Placed};
use crate::units::dead::Dead;
use crate::units::owner::Owner;
use crate::units::relations::Relations;
use crate::units::tag_book::TagBook;
use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;
use crate::values::attitude::Attitude;
use crate::values::rank::Rank;

/// The living units whose held modifiers `HeldPass::run` writes.
type HeldUnits<'w, 's> = Query<
    'w,
    's,
    (
        &'static StableId,
        &'static Position,
        &'static Team,
        Option<&'static UnitTags>,
        Option<&'static Owner>,
        (&'static mut Modifiers, &'static mut ModifierClocks),
        Option<&'static Body>,
        Entity,
    ),
    Without<Dead>,
>;

/// The modifiers held on units each tick: by auras, by other capabilities, and by players.
#[derive(Debug)]
pub(crate) struct HeldPass;

impl HeldPass {
    /// Holds, in Resolve each tick, each aura's modifier on every living unit whose body comes
    /// within its radius of its carrier's position in the map's metric, as an area's reaches, that
    /// its `affects` selects, from the unit that carries the aura. An aura's instance projects only
    /// while it has a stack and takes effect on its carrier, which its immunities may suppress.
    /// Each one another capability holds this tick, as `HeldModifiers` lists it; and each player
    /// modifier on every living unit of its player that the modifier's `affects` selects, from no
    /// source. Each ends on a unit that left it. An aura's modifier resolves its numbers from the
    /// ability that gave the aura, a player modifier's at rank 1; neither has a duration.
    pub(crate) fn run(
        (book, stats, (tick, rate), metric, players, mut others): (
            Res<'_, ModifierBook>,
            Option<Res<'_, StatBook>>,
            (Res<'_, SimTick>, Res<'_, TickRate>),
            Res<'_, Metric>,
            Res<'_, PlayerModifiers>,
            ResMut<'_, HeldModifiers>,
        ),
        params: Res<'_, ParamBook>,
        sources: ParamSources<'_, '_>,
        relations: Res<'_, Relations>,
        mut units: HeldUnits<'_, '_>,
        tag_book: Option<Res<'_, TagBook>>,
        (mut held, mut grid): (Local<'_, Vec<Held>>, Local<'_, BodyGrid<Entity>>),
    ) {
        if stats.is_none() {
            return;
        }
        let rate = *rate;
        held.clear();
        held.extend(others.0.drain(..));
        HeldPass::hold_player_modifiers(&mut held, &book, &players, &units);
        let granting = tag_book
            .as_deref()
            .map_or(TagSet::default(), TagBook::granting);
        let mut indexed = false;
        for (&source, &at, &team, carrier, _, (modifiers, _), ..) in &units {
            let immune = carrier.map_or(TagSet::default(), |tags| tags.immune);
            let takes_effect = TagBook::effect_test(granting, immune);
            for instance in modifiers.iter() {
                let (Some(aura), Some(radius)) =
                    (&book.get(instance.id).spec.aura, instance.aura_radius)
                else {
                    continue;
                };
                if instance.stacks == 0 || !takes_effect(book.tags(instance.id)) {
                    continue;
                }
                if !indexed {
                    let placed = units.iter().map(|(&id, &at, .., body, entity)| Placed {
                        id,
                        key: entity,
                        at,
                        shape: Body::shape_of(body),
                    });
                    grid.rebuild(placed);
                    indexed = true;
                }
                let (filter, modifier) = (aura.affects, aura.modifier);
                grid.visit_near(at, radius, |body| {
                    let (&target, _, &other, tags, ..) =
                        units.get(body.key).expect("an indexed unit");
                    let tags = tags.map_or(TagSet::default(), |tags| tags.tags);
                    let attitude = relations.between(team, other);
                    let reaches = metric.reaches(at, Shape::POINT, radius, body.at, body.shape);
                    if reaches && filter.selects(attitude, tags) {
                        held.push(Held {
                            target,
                            modifier,
                            source: Some(source),
                            ability: instance.ability,
                            rank: instance.rank,
                        });
                    }
                });
            }
        }
        held.sort_unstable();
        for (&id, _, _, _, _, (modifiers, clocks), ..) in &mut units {
            let mut carried = CarriedMut::new(modifiers, clocks);
            let first = held.partition_point(|entry| entry.target < id);
            let mine = held[first..].iter().take_while(|entry| entry.target == id);
            let kept = |modifier, source| {
                mine.clone()
                    .any(|entry| entry.modifier == modifier && entry.source == source)
            };
            carried.release_held(kept);
            for entry in mine {
                match carried.modifiers().get(entry.modifier, entry.source) {
                    Some(instance) if instance.lifetime.held_by(Hold::Held) => continue,
                    Some(_) => {
                        carried.hold(entry.modifier, entry.source, Hold::Held);
                        continue;
                    }
                    None => {}
                }
                let applier = Applier {
                    source: entry.source,
                    ability: entry.ability,
                    rank: entry.rank,
                    hold: Some(Hold::Held),
                };
                let source = entry.source.and_then(|source| sources.get(source));
                let param = |place: &ParamPlace| {
                    let (ability, rank) = (entry.ability, entry.rank);
                    params.modifier_param(entry.modifier, ability, rank, place, source.as_ref())
                };
                let start = tick.start();
                carried.apply(book.application(entry.modifier, applier, None, start, rate, param));
            }
        }
    }

    /// Adds to `held` each player modifier on every one of `units` of its player that the
    /// modifier's `affects` selects, from no source, at rank 1.
    fn hold_player_modifiers(
        held: &mut Vec<Held>,
        book: &ModifierBook,
        players: &PlayerModifiers,
        units: &HeldUnits<'_, '_>,
    ) {
        for (&target, _, _, tags, owner, ..) in units {
            let Some(owner) = owner else {
                continue;
            };
            let tags = tags.map_or(TagSet::default(), |tags| tags.tags);
            for modifier in players.of(owner.slot()) {
                let affects = book.get(modifier).spec.affects;
                if affects.is_none_or(|filter| filter.selects(Attitude::Friendly, tags)) {
                    held.push(Held {
                        target,
                        modifier,
                        source: None,
                        ability: None,
                        rank: Rank::FIRST,
                    });
                }
            }
        }
    }
}
