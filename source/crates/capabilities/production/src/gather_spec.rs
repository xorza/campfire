use std::num::NonZeroU32;

use campfire_actions::LoadedAction;
use campfire_common::Ticks;
use campfire_declarations::{BookCx, BookError, ById, ByIdBuilder, PerId};
use campfire_math::Num;
use campfire_players::ResourceBook;
use campfire_values::{ActionId, Distance, ResourceId, TimeTooLarge};

use crate::Production;
use crate::error::ProductionDataError;
use crate::gather_data::GatherData;

/// What a gather action runs: the player resource it gathers, the most a trip carries, what each
/// windup takes of it, at most `take`, how far from its node it looks for another, and how long
/// its worker stands at a drop-off before its load joins its player's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatherSpec {
    pub resource: ResourceId,
    pub take: NonZeroU32,
    pub unit: NonZeroU32,
    pub bounce: Num,
    pub deliver: Ticks,
}

impl PerId<ActionId> for GatherSpec {
    const SCRIPTS_ONLY: bool = false;
}

impl GatherSpec {
    /// What each gather gathers, carries, takes at each windup, looks for another node within and
    /// stands at a drop-off for; an error for a resource the mode does not declare, a unit past
    /// the take, or a time too large to count in ticks.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<ActionId, Self>, BookError> {
        let resources = cx.book_if::<ResourceBook>()?;
        let mut book = ByIdBuilder::default();
        for action in LoadedAction::all(cx)? {
            let Some(data) = action.section::<GatherData>() else {
                continue;
            };
            let resource = Production::resource(resources, &data.resource);
            let unit = data.unit.unwrap_or(data.take);
            if unit > data.take {
                return Err(action.broken(ProductionDataError::UnitPastTake));
            }
            let deliver = cx.rate().ticks(data.deliver_ms.unwrap_or(0));
            book.set(
                action.action,
                Self {
                    resource: resource.map_err(|name| action.unknown(name))?,
                    take: data.take,
                    unit,
                    bounce: data.bounce.map_or(Num::ZERO, Distance::get),
                    deliver: deliver.ok_or_else(|| action.broken(TimeTooLarge))?,
                },
            );
        }
        Ok(book.build())
    }
}
