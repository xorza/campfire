use bevy_ecs::query::QueryEntityError;
use bevy_ecs::system::{Query, Res, ResMut};
use campfire_sim::TickClock;
use campfire_values::Rank;

use crate::application::{Adding, ApplyAt};
use crate::applier::Applier;
use crate::carried_mut::CarriedMut;
use crate::grants::{Grant, GrantChange, Grants};
use crate::modifier_book::ModifierBook;
use crate::modifier_data::Reapply;
use crate::modifier_dues::ModifierIndexes;
use crate::modifier_params::ModifierParams;
use crate::modifiers::Modifiers;
use crate::modifiers::modifier_clocks::ModifierClocks;

/// The one pass that keeps on each unit the modifiers its grant sources want from it, each at the
/// rank its holds agree on (04-capabilities/stats.md, Passives by one pass).
#[derive(Debug)]
pub struct GrantPass;

impl GrantPass {
    /// Applies the stage's grants, unit by unit in the order the sources pushed them: a grant at
    /// a rank the unit does not hold its modifier at, by its hold, applies at that rank, and one
    /// granted no more is released. The params are the match's param book's, of the unit as it
    /// is now. A unit despawned since its source pushed holds nothing, and is passed over.
    pub(crate) fn run(
        book: Res<'_, ModifierBook>,
        clock: TickClock<'_>,
        params: ModifierParams<'_, '_>,
        mut grants: ResMut<'_, Grants>,
        mut units: Query<'_, '_, (&mut Modifiers, &mut ModifierClocks)>,
        mut indexes: ModifierIndexes<'_>,
    ) {
        let at = ApplyAt {
            now: clock.start(),
            tick_rate: clock.rate(),
            params: params.book(),
        };
        for unit in &*grants {
            let (modifiers, clocks) = match units.get_mut(unit.entity) {
                Ok(carried) => carried,
                // A sync point between the push and the pass applied its despawn.
                Err(QueryEntityError::NotSpawned(_)) => continue,
                Err(error) => {
                    panic!("a source pushes the grants of a unit that carries modifiers: {error:?}")
                }
            };
            let dues = indexes.of(unit.unit, unit.entity);
            let mut carried = CarriedMut::new(modifiers, clocks, dues);
            // The unit's params, resolved at its first apply, as a release reads none.
            let mut source = None;
            for &grant in unit.grants {
                let Grant {
                    modifier,
                    hold,
                    action,
                    ..
                } = grant;
                let from = grant.source(unit.unit);
                let mut add = |carried: &mut CarriedMut<'_>, rank, reapply, kept| {
                    let applier = Applier {
                        source: from,
                        action,
                        rank,
                        hold: Some(hold),
                    };
                    let source = match from {
                        Some(_) => source
                            .get_or_insert_with(|| params.source(unit.unit))
                            .as_ref(),
                        None => None,
                    };
                    let adding = Adding {
                        id: modifier,
                        applier,
                        source,
                        duration: None,
                    };
                    let mut application = book.application(at, adding);
                    if let Some(reapply) = reapply {
                        application.reapply = reapply;
                    }
                    if let Some(kept) = kept {
                        application.instance.lifetime = kept;
                    }
                    carried.apply(application);
                };
                match grant.change(carried.modifiers(), unit.unit) {
                    Some(GrantChange::Apply(rank)) => add(&mut carried, rank, None, None),
                    Some(GrantChange::Rank(rank)) => {
                        add(&mut carried, rank, Some(Reapply::Refresh), None);
                    }
                    Some(GrantChange::Join) => {
                        let rank = carried
                            .modifiers()
                            .get(modifier, from)
                            .map_or(Rank::FIRST, |held| held.instance.rank);
                        add(&mut carried, rank, Some(Reapply::Ignore), None);
                    }
                    Some(GrantChange::Release) => {
                        carried.release(modifier, from, hold);
                        // The last hold that asked a rank of its own gone, the holds left agree
                        // on the first, and the instance takes it, its lifetime as it stands.
                        let lowered = carried.modifiers().get(modifier, from).filter(|held| {
                            let instance = held.instance;
                            hold.asks_rank()
                                && instance.lifetime.held_only()
                                && !instance.lifetime.ranked_besides(hold)
                                && instance.rank > Rank::FIRST
                        });
                        if let Some(held) = lowered {
                            let kept = held.instance.lifetime;
                            add(
                                &mut carried,
                                Rank::FIRST,
                                Some(Reapply::Refresh),
                                Some(kept),
                            );
                        }
                    }
                    None => {}
                }
            }
        }
        grants.clear();
    }
}
