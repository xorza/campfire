use std::collections::BTreeMap;

use campfire_common::{DeclaredName, Ticks};
use campfire_declarations::{
    BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId, Section, SectionTable,
};
use campfire_players::{ResourceAmount, ResourceBook};
use campfire_script::{ApiVersion, DataSection, DataTable, Status};
use campfire_values::{TimeTooLarge, UnitType};
use serde::{Deserialize, Serialize};

use crate::Production;

/// A unit type's `sell` section, a building its player may sell (04-capabilities/production.md, A
/// building may be sold): what its sale refunds of each player resource, and the milliseconds it
/// takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SellData {
    pub refund: BTreeMap<DeclaredName, u32>,
    pub time_ms: u32,
}

/// A type's sale as the match runs it: its refund of each player resource, and its time in ticks,
/// rounded up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SellSpec {
    pub refund: Box<[ResourceAmount]>,
    pub time: Ticks,
}

impl SellSpec {
    /// The sale of each unit type that stands with a `sell` section, its resources resolved; an
    /// error for a resource the mode does not declare, or a time too large to count in ticks.
    pub(crate) fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let resources = cx.book_if::<ResourceBook>()?;
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<SellData>() else {
                continue;
            };
            let at = || BookPlace::UnitType(unit.place());
            let refund = data.refund.iter().map(|(name, &amount)| {
                let resource = Production::resource(resources, name)
                    .map_err(|name| BookError::unknown(unit.package, at(), name))?;
                Ok(ResourceAmount {
                    resource,
                    amount: i64::from(amount),
                })
            });
            let time = cx.rate().ticks(u64::from(data.time_ms));
            let time = time.ok_or_else(|| BookError::rule(unit.package, at(), TimeTooLarge))?;
            let spec = Self {
                refund: refund.collect::<Result<_, BookError>>()?,
                time,
            };
            book.set(unit.unit_type, spec);
        }
        Ok(book.build())
    }
}

impl PerId<UnitType> for SellSpec {
    const SCRIPTS_ONLY: bool = false;
}

impl DataSection for SellData {
    const TABLE: DataTable = DataTable::new("A unit type's `sell`");
}

impl Section for SellData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "sell";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
