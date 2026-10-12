use bevy_ecs::entity::Entity;
use bevy_ecs::query::{With, Without};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_sim::{Position, StableId, TickClock, Unpredicted};

use crate::geometry::metric::Metric;
use crate::geometry::shape::Shape;
use crate::players::on_match::OnMatch;
use crate::players::player_rows::PlayerRows;
use crate::stats::applier::Applier;
use crate::stats::carried_mut::CarriedMut;
use crate::stats::held_modifiers::{Held, HeldModifiers};
use crate::stats::lifetime::Hold;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifier_params::ModifierParams;
use crate::stats::modifier_spec::ParamPlace;
use crate::stats::modifiers::Modifiers;
use crate::stats::player_modifiers::PlayerModifiers;
use crate::stats::stat_books::StatBooks;
use crate::units::body::Body;
use crate::units::body_grid::{BodyGrid, Placed};
use crate::units::dead::Dead;
use crate::units::owner::Owner;
use crate::units::relations::Relations;
use crate::units::tag_book::TagBook;
use crate::units::tag_effect::TagEffect;
use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;
use crate::values::rank::Rank;
use crate::values::relation::Relation;

/// The living units whose held modifiers `HeldPass::run` writes: on a predicting client, those it
/// predicts.
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
    (Without<Dead>, Without<Unpredicted>),
>;

/// The living units a predicting client holds as the server sent them: their auras reach the
/// units it predicts, but the pass writes none of their modifiers.
type HeldCarriers<'w, 's> = Query<
    'w,
    's,
    (
        &'static StableId,
        &'static Position,
        &'static Team,
        Option<&'static UnitTags>,
        &'static Modifiers,
    ),
    (With<Unpredicted>, Without<Dead>),
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
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    pub(crate) fn run(
        books: StatBooks<'_>,
        clock: TickClock<'_>,
        metric: Res<'_, Metric>,
        players: PlayerRows<'_, '_, PlayerModifiers>,
        mut others: ResMut<'_, HeldModifiers>,
        params: ModifierParams<'_, '_>,
        relations: OnMatch<'_, '_, Relations>,
        mut units: HeldUnits<'_, '_>,
        carriers: HeldCarriers<'_, '_>,
        mut held: Local<'_, Vec<Held>>,
        mut grid: Local<'_, BodyGrid<Entity>>,
    ) {
        let StatBooks {
            stats,
            modifiers: book,
            tags: tag_book,
            ..
        } = books;
        if stats.is_none() {
            return;
        }
        let relations = relations.get();
        let rate = clock.rate();
        held.clear();
        held.extend(others.0.drain(..));
        HeldPass::hold_player_modifiers(&mut held, &book, &players, &units);
        let granting = tag_book
            .as_deref()
            .map_or(TagSet::default(), TagBook::granting);
        let mut indexed = false;
        let written = units
            .iter()
            .map(|(id, at, team, tags, _, (modifiers, _), ..)| (id, at, team, tags, modifiers));
        for (&source, &at, &team, carrier, modifiers) in written.chain(&carriers) {
            let immune = carrier.map_or(TagSet::default(), |tags| tags.immune);
            let takes_effect = TagEffect::new(granting, immune);
            for instance in modifiers.iter() {
                let (Some(aura), Some(radius)) =
                    (&book.get(instance.id).spec.aura, instance.aura_radius)
                else {
                    continue;
                };
                if instance.stacks == 0 || !takes_effect.takes(book.tags(instance.id)) {
                    continue;
                }
                if !indexed {
                    let placed = units.iter().map(|(&id, &at, .., body, entity)| Placed {
                        id,
                        key: entity,
                        pos: at,
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
                    let relation = relations.between(team, other);
                    let reaches = metric.reaches(at, Shape::POINT, radius, body.pos, body.shape);
                    if reaches && filter.selects(relation, tags) {
                        held.push(Held {
                            target,
                            modifier,
                            source: Some(source),
                            action: instance.action,
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
                    action: entry.action,
                    rank: entry.rank,
                    hold: Some(Hold::Held),
                };
                let source = entry.source.and_then(|source| params.source(source));
                let param = |place: &ParamPlace| {
                    let (action, rank) = (entry.action, entry.rank);
                    params.modifier_param(entry.modifier, action, rank, place, source.as_ref())
                };
                let start = clock.start();
                carried.apply(book.application(entry.modifier, applier, None, start, rate, param));
            }
        }
    }

    /// Adds to `held` each player modifier on every one of `units` of its player that the
    /// modifier's `affects` selects, from no source, at rank 1.
    fn hold_player_modifiers(
        held: &mut Vec<Held>,
        book: &ModifierBook,
        players: &PlayerRows<'_, '_, PlayerModifiers>,
        units: &HeldUnits<'_, '_>,
    ) {
        for (&target, _, _, tags, owner, ..) in units {
            let Some(held_by) = players.of(owner) else {
                continue;
            };
            let tags = tags.map_or(TagSet::default(), |tags| tags.tags);
            for modifier in held_by.iter() {
                let affects = book.get(modifier).spec.affects;
                if affects.is_none_or(|filter| filter.selects(Relation::Friendly, tags)) {
                    held.push(Held {
                        target,
                        modifier,
                        source: None,
                        action: None,
                        rank: Rank::FIRST,
                    });
                }
            }
        }
    }
}
