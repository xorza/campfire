use std::ops::ControlFlow;

use bevy_ecs::entity::Entity;
use bevy_ecs::entity_disabling::Disabled;
use bevy_ecs::query::{Allow, Has, With, Without};
use bevy_ecs::system::{Local, Query, Res, ResMut, SystemParam};
use campfire_declarations::RoleForm;
use campfire_geometry::Placed;
use campfire_players::OnMatch;
use campfire_sim::{EntityIndex, Position, StableId, TickClock, Unpredicted};
use campfire_units::{Dead, Life, MapBook, Owner, TagEffect, UnitsNear};
use campfire_values::{Relations, TagSet, Team, UnitTags};

use crate::application::{Adding, ApplyAt};
use crate::applier::Applier;
use crate::carried_mut::CarriedMut;
use crate::held_modifiers::{Held, HeldModifiers};
use crate::lifetime::Hold;
use crate::modifier_carriers::ModifierCarriers;
use crate::modifier_dues::ModifierIndexes;
use crate::modifier_params::ModifierParams;
use crate::modifiers::Modifiers;
use crate::modifiers::modifier_clocks::ModifierClocks;
use crate::projection::{Affects, Projection};
use crate::stat_books::StatBooks;

/// The living units whose held modifiers `HeldPass::run` writes, riders out of the world among
/// them, which their containers' modifiers reach (transports.md, A container's modifier holds on
/// its riders): on a predicting client, those it predicts.
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
        Entity,
        Option<&'static Life>,
    ),
    (Without<Dead>, Without<Unpredicted>, Allow<Disabled>),
>;

/// The living units a predicting client holds as the server sent them: their auras reach the
/// units it predicts, but the pass writes none of their modifiers.
type SentCarriers<'w, 's> = Query<
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
/// hold, as a mode hides them from it, and those it does not hold at all
/// (10-replication-audiences.md, A mode's hidden data is not predicted).
#[derive(SystemParam, Debug)]
pub(crate) struct UnseenSources<'w, 's> {
    index: Res<'w, EntityIndex>,
    modifiers: Query<'w, 's, Has<Modifiers>, Allow<Unpredicted>>,
}

/// The modifiers held on units each tick: by auras, and by other capabilities.
#[derive(Debug)]
pub struct HeldPass;

impl HeldPass {
    /// Holds, in Resolve each tick, each aura's modifier on every living unit whose body comes
    /// within its radius of its carrier's position in the map's metric, that its `affects`
    /// selects, from the unit that carries the aura. An aura's instance projects only while it has
    /// a stack and takes effect on its carrier, which its immunities may suppress. And each one
    /// another capability holds this tick, as `HeldModifiers` lists it: on its unit, or on the
    /// units its projection selects, by the rule of an aura's. Each ends on a unit that left it.
    /// An aura's modifier resolves its numbers from the ability that gave the aura, and has no
    /// duration. A predicting client keeps a held instance from a source whose modifiers it does
    /// not hold, as a mode that hides them from it, or that it does not hold at all, as the server
    /// sent it: it cannot tell whether that source's aura still reaches, so the server's word
    /// corrects it once as it comes and once as it goes (10-replication-audiences.md, A mode's
    /// hidden data is not predicted).
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    pub(crate) fn run<F: RoleForm>(
        books: StatBooks<'_>,
        clock: TickClock<'_>,
        map: Res<'_, MapBook>,
        mut others: ResMut<'_, HeldModifiers>,
        params: ModifierParams<'_, '_>,
        relations: OnMatch<'_, '_, Relations>,
        mut units: HeldUnits<'_, '_>,
        sent: SentCarriers<'_, '_>,
        unseen: UnseenSources<'_, '_>,
        (mut held, mut projections, mut visits): (
            Local<'_, Vec<Held>>,
            Local<'_, Vec<Projection>>,
            Local<'_, Vec<Entity>>,
        ),
        near: UnitsNear<'_>,
        carriers: Res<'_, ModifierCarriers>,
        mut indexes: ModifierIndexes<'_>,
    ) {
        let StatBooks {
            modifiers: book,
            tags: tag_book,
            ..
        } = books;
        let relations = relations.get();
        let at = ApplyAt {
            now: clock.start(),
            tick_rate: clock.rate(),
            params: params.book(),
        };
        held.clear();
        held.extend(others.held.drain(..));
        projections.clear();
        projections.extend(others.projections.drain(..));
        let granting = tag_book.granting();
        let living = carriers.auras().filter_map(|entity| {
            let written = units
                .get(entity)
                .ok()
                .map(|(id, at, team, tags, _, (modifiers, _), ..)| (id, at, team, tags, modifiers));
            written.or_else(|| sent.get(entity).ok())
        });
        for (&source, at, &team, carrier, modifiers) in living {
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
                projections.push(Projection {
                    at: at.get(),
                    radius,
                    team,
                    affects: Affects::Filter(aura.affects),
                    modifier: aura.modifier,
                    source,
                    action: instance.action,
                    rank: instance.rank,
                });
            }
        }
        let grid = near.current();
        for projection in &*projections {
            grid.visit_near(projection.at, projection.radius, |entry| {
                let Ok((&target, _, &other, tags, .., life)) = units.get(entry.key) else {
                    return ControlFlow::Continue(());
                };
                let tags = tags.map_or_else(TagSet::default, |tags| tags.tags);
                let relation = relations.between(projection.team, other);
                let at = Placed::point(projection.at);
                if campfire_units::lives(false, life)
                    && map.metric().reaches(at, projection.radius, entry.body)
                    && projection.selects(target, relation, tags)
                {
                    held.push(Held {
                        target,
                        modifier: projection.modifier,
                        source: Some(projection.source),
                        action: projection.action,
                        rank: projection.rank,
                    });
                }
                ControlFlow::Continue(())
            });
        }
        held.sort_unstable();
        // The units held a modifier this tick, and those that carry a hold to let go of.
        visits.clear();
        visits.extend(carriers.held());
        visits.extend(
            held.iter()
                .filter_map(|entry| unseen.index.get(entry.target)),
        );
        visits.sort_unstable();
        visits.dedup();
        for &entity in &*visits {
            let Ok((&id, _, _, _, _, (modifiers, clocks), entity, _)) = units.get_mut(entity)
            else {
                continue;
            };
            let mut carried = CarriedMut::new(modifiers, clocks, indexes.of(id, entity));
            let first = held.partition_point(|entry| entry.target < id);
            let mine = held[first..].iter().take_while(|entry| entry.target == id);
            let kept = |modifier, source| {
                unseen.holds::<F>(source)
                    || mine
                        .clone()
                        .any(|entry| entry.modifier == modifier && entry.source == source)
            };
            carried.release_held(kept);
            for entry in mine {
                let held = carried.modifiers().get(entry.modifier, entry.source);
                if held.is_some_and(|instance| instance.lifetime.held_by(Hold::Held)) {
                    continue;
                }
                let applier = Applier {
                    source: entry.source,
                    action: entry.action,
                    rank: entry.rank,
                    hold: Some(Hold::Held),
                };
                let source = entry.source.and_then(|source| params.source(source));
                let adding = Adding {
                    id: entry.modifier,
                    applier,
                    source: source.as_ref(),
                    duration: None,
                };
                carried.add(&book, at, adding);
            }
        }
    }
}

impl UnseenSources<'_, '_> {
    /// Whether an instance from `source` stays as the server sent it: in a predicting client's
    /// form `F`, a source it cannot see the aura of.
    fn holds<F: RoleForm>(&self, source: Option<StableId>) -> bool {
        F::PREDICTS
            && source.is_some_and(|source| {
                let entity = self.index.get(source);
                !entity.is_some_and(|entity| self.modifiers.get(entity).unwrap_or(false))
            })
    }
}
