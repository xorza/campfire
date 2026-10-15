use bevy_ecs::query::QueryEntityError;
use bevy_ecs::system::{Query, Res, ResMut};
use campfire_sim::TickClock;

use crate::applier::Applier;
use crate::carried_mut::CarriedMut;
use crate::grants::{Grant, GrantChange, Grants};
use crate::modifier_book::ModifierBook;
use crate::modifier_params::ModifierParams;
use crate::modifier_spec::ParamPlace;
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
    ) {
        let now = clock.start();
        for unit in &*grants {
            let (modifiers, clocks) = match units.get_mut(unit.entity) {
                Ok(carried) => carried,
                // A sync point between the push and the pass applied its despawn.
                Err(QueryEntityError::NotSpawned(_)) => continue,
                Err(error) => {
                    panic!("a source pushes the grants of a unit that carries modifiers: {error:?}")
                }
            };
            let mut carried = CarriedMut::new(modifiers, clocks);
            let source = params.source(unit.unit);
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
                            source: Some(unit.unit),
                            action,
                            rank,
                            hold: Some(hold),
                        };
                        let param = |place: ParamPlace| {
                            params.modifier_param(modifier, action, rank, place, source.as_ref())
                        };
                        carried.apply(book.application(
                            modifier,
                            applier,
                            None,
                            now,
                            clock.rate(),
                            param,
                        ));
                    }
                    Some(GrantChange::Release) => {
                        carried.release(modifier, Some(unit.unit), hold);
                    }
                    None => {}
                }
            }
        }
        grants.clear();
    }
}
