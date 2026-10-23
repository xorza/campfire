use bevy_ecs::query::QueryEntityError;
use bevy_ecs::system::{Query, Res, ResMut};
use campfire_sim::TickClock;

use crate::application::{Adding, ApplyAt};
use crate::applier::Applier;
use crate::carried_mut::CarriedMut;
use crate::grants::{Grant, GrantChange, Grants};
use crate::modifier_book::ModifierBook;
use crate::modifier_dues::ModifierIndexes;
use crate::modifier_params::ModifierParams;
use crate::modifiers::Modifiers;
use crate::modifiers::modifier_clocks::ModifierClocks;

/// The one pass that keeps on each unit the modifiers its grant sources want from it: its
/// actions' passives and holds, and its carried items'.
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
                match grant.change(carried.modifiers(), unit.unit) {
                    Some(GrantChange::Apply(rank)) => {
                        let applier = Applier {
                            source: grant.source(unit.unit),
                            action,
                            rank,
                            hold: Some(hold),
                        };
                        let source = match grant.source(unit.unit) {
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
                        carried.add(&book, at, adding);
                    }
                    Some(GrantChange::Release) => {
                        carried.release(modifier, grant.source(unit.unit), hold);
                    }
                    None => {}
                }
            }
        }
        grants.clear();
    }
}
