use bevy_ecs::query::{Allow, Has, With, Without};
use bevy_ecs::system::{Local, Query, Res, ResMut, SystemParam};
use campfire_geometry::Shape;
use campfire_players::{OnMatch, PlayerRows};
use campfire_sim::{EntityIndex, Position, StableId, TickClock, Unpredicted};
use campfire_units::{Dead, MapBook, Owner, Predicting, TagEffect, UnitsNear};
use campfire_values::{Rank, Relation, Relations, TagSet, Team, UnitTags};

use crate::applier::Applier;
use crate::carried_mut::CarriedMut;
use crate::held_modifiers::{Held, HeldModifiers};
use crate::lifetime::Hold;
use crate::modifier_book::ModifierBook;
use crate::modifier_params::ModifierParams;
use crate::modifier_spec::ParamPlace;
use crate::modifiers::Modifiers;
use crate::modifiers::modifier_clocks::ModifierClocks;
use crate::player_modifiers::PlayerModifiers;
use crate::stat_books::StatBooks;

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

/// The sources whose auras a predicting client cannot see: the units whose modifiers it does not
/// hold, as a mode hides them from it, and those it does not hold at all (design 14, D5).
#[derive(SystemParam, Debug)]
pub(crate) struct UnseenSources<'w, 's> {
    predicting: Option<Res<'w, Predicting>>,
    index: Res<'w, EntityIndex>,
    modifiers: Query<'w, 's, Has<Modifiers>, Allow<Unpredicted>>,
}

/// The modifiers held on units each tick: by auras, by other capabilities, and by players.
#[derive(Debug)]
pub struct HeldPass;

impl HeldPass {
    /// Holds, in Resolve each tick, each aura's modifier on every living unit whose body comes
    /// within its radius of its carrier's position in the map's metric, as an area's reaches, that
    /// its `affects` selects, from the unit that carries the aura. An aura's instance projects only
    /// while it has a stack and takes effect on its carrier, which its immunities may suppress.
    /// Each one another capability holds this tick, as `HeldModifiers` lists it; and each player
    /// modifier on every living unit of its player that the modifier's `affects` selects, from no
    /// source. Each ends on a unit that left it. An aura's modifier resolves its numbers from the
    /// ability that gave the aura, a player modifier's at rank 1; neither has a duration. A
    /// predicting client keeps a held instance from a source whose modifiers it does not hold, as
    /// a mode that hides them from it, or that it does not hold at all, as the server sent it: it
    /// cannot tell whether that source's aura still reaches, so the server's word corrects it once
    /// as it comes and once as it goes (design 14, D5).
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    pub(crate) fn run(
        books: StatBooks<'_>,
        clock: TickClock<'_>,
        map: Res<'_, MapBook>,
        players: PlayerRows<'_, '_, PlayerModifiers>,
        mut others: ResMut<'_, HeldModifiers>,
        params: ModifierParams<'_, '_>,
        relations: OnMatch<'_, '_, Relations>,
        mut units: HeldUnits<'_, '_>,
        carriers: HeldCarriers<'_, '_>,
        unseen: UnseenSources<'_, '_>,
        mut held: Local<'_, Vec<Held>>,
        mut near: UnitsNear<'_, '_>,
    ) {
        let StatBooks {
            modifiers: book,
            tags: tag_book,
            ..
        } = books;
        let relations = relations.get();
        let rate = clock.rate();
        held.clear();
        held.extend(others.0.drain(..));
        HeldPass::hold_player_modifiers(&mut held, &book, &players, &units);
        let granting = tag_book.granting();
        let written = units
            .iter()
            .map(|(id, at, team, tags, _, (modifiers, _), ..)| (id, at, team, tags, modifiers));
        for (&source, &at, &team, carrier, modifiers) in written.chain(&carriers) {
            let immune = carrier.map_or_else(TagSet::default, |tags| tags.immune);
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
                let (filter, modifier) = (aura.affects, aura.modifier);
                near.current().visit_near(at, radius, |body| {
                    let Ok((&target, _, &other, tags, ..)) = units.get(body.key) else {
                        return;
                    };
                    let tags = tags.map_or_else(TagSet::default, |tags| tags.tags);
                    let relation = relations.between(team, other);
                    let reaches =
                        map.metric
                            .reaches(at, Shape::POINT, radius, body.pos, body.shape);
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
                unseen.holds(source)
                    || mine
                        .clone()
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
            let tags = tags.map_or_else(TagSet::default, |tags| tags.tags);
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

impl UnseenSources<'_, '_> {
    /// Whether an instance from `source` stays as the server sent it: on a predicting client, a
    /// source it cannot see the aura of.
    fn holds(&self, source: Option<StableId>) -> bool {
        self.predicting.is_some()
            && source.is_some_and(|source| {
                let entity = self.index.get(source);
                !entity.is_some_and(|entity| self.modifiers.get(entity).unwrap_or(false))
            })
    }
}
