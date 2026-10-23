use campfire_common::Ticks;
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_players::ResourceBook;
use campfire_script::UnknownName;
use campfire_stats::StatBook;
use campfire_units::StatId;
use campfire_values::{Named, ResourceId, UnitType};

use crate::error::IncomeError;
use crate::income_data::{IncomeData, IncomePays};

/// A unit type's income as the match runs it (04-capabilities/games.md, Income over time): its
/// resource, what a period pays, its period in ticks, and what a capture pays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TypeIncome {
    pub(crate) resource: ResourceId,
    pub(crate) pays: TypePays,
    pub(crate) period: Ticks,
    pub(crate) capture: u32,
}

/// What a period of a type's income pays: its amount, or the unit's stat, rounded down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypePays {
    Amount(u32),
    Stat(StatId),
}

impl TypeIncome {
    /// The income of each unit type that stands with one, its names resolved; an error for a
    /// resource or a stat the match does not have, and for a period of more ticks than a count
    /// holds.
    pub(crate) fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let resources = cx.book::<ResourceBook>()?;
        let stats = cx.book_if::<StatBook>()?;
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<IncomeData>() else {
                continue;
            };
            let at = || BookPlace::UnitType(unit.place());
            let resource = resources.0.named(data.resource.as_str()).ok_or_else(|| {
                let name = UnknownName::new(ResourceId::NAMES, &data.resource);
                BookError::unknown(unit.package, at(), name)
            })?;
            let pays = match &data.pays {
                IncomePays::Amount(amount) => TypePays::Amount(*amount),
                IncomePays::AmountStat(stat) => {
                    let stats = stats.ok_or_else(|| {
                        BookError::unknown(
                            unit.package,
                            at(),
                            UnknownName::new(StatId::NAMES, stat),
                        )
                    })?;
                    TypePays::Stat(stats.named_in(unit.package, at(), stat)?)
                }
            };
            let period = cx
                .rate()
                .duration(data.period_ms)
                .ok_or_else(|| BookError::rule(unit.package, at(), IncomeError::Period))?;
            let income = Self {
                resource,
                pays,
                period,
                capture: data.capture,
            };
            book.set(unit.unit_type, income);
        }
        Ok(book.build())
    }
}

impl PerId<UnitType> for TypeIncome {
    const SCRIPTS_ONLY: bool = true;
}
