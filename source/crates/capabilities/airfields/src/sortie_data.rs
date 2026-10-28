use std::num::NonZeroI64;

use campfire_common::{DeclaredName, Ticks};
use campfire_declarations::{
    BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId, Section, SectionTable,
};
use campfire_math::{Num, Rounding};
use campfire_navigation::FlightData;
use campfire_script::{ApiVersion, DataSection, DataTable, Status, UnknownName};
use campfire_sim::MatchRate;
use campfire_values::{DamageKind, Named, UnitType};
use serde::{Deserialize, Serialize};

use crate::error::AirfieldError;

/// A unit type's `sortie` section, a unit an airfield parks: the milliseconds it waits at its
/// runway's start before it rolls, the meters a second its roll gains a second
/// (04-capabilities/airfields.md, A jet takes off as a jet), its speed in meters a second on the
/// ground as it taxis, the
/// milliseconds it stands idle in the air before it goes home, none for never, and what it loses
/// stranded, none for nothing (04-capabilities/airfields.md, A jet takes off and lands on its
/// runway; 04-capabilities/airfields.md, A jet goes home).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SortieData {
    pub pause_ms: u32,
    pub acceleration: Num,
    pub taxi_speed: Num,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stranded: Option<StrandedData>,
}

/// A sortie's `stranded`: the share of its unit's whole life it loses each second it circles with
/// no airfield to go to, as damage of the mode's damage kind `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrandedData {
    pub share: Num,
    pub kind: DeclaredName,
}

/// A sortie at the match's rate: its pause and its idle time in ticks, rounded up, the step its
/// roll gains a tick, its taxi's step a tick, and its stranded loss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortieSpec {
    pub pause: Ticks,
    pub acceleration: Num,
    pub taxi_step: Num,
    pub idle: Option<Ticks>,
    pub stranded: Option<StrandedLoss>,
}

/// A sortie's stranded loss with its kind resolved: the share of a whole life a second, and the
/// damage kind it is dealt as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrandedLoss {
    pub share: Num,
    pub kind: DamageKind,
}

impl SortieSpec {
    /// The sortie of each unit type that stands with a `sortie` section, at the match's rate; an
    /// error for a type of no `flight`, which nothing flies, for an acceleration that gains no step
    /// a tick, for a stranded loss of no share of a life above 0 to 1, and for one of a damage kind
    /// the match does not have.
    pub(crate) fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let rate = cx.rate();
        let kinds = cx.names_if::<DamageKind>()?;
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<SortieData>() else {
                continue;
            };
            let at = || BookPlace::UnitType(unit.place());
            let broken = |problem| BookError::rule(unit.package, at(), problem);
            if unit.data.section::<FlightData>().is_none() {
                return Err(broken(AirfieldError::SortieFlight));
            }
            let stranded = data
                .stranded
                .as_ref()
                .map(|stranded| {
                    if stranded.share <= Num::ZERO || stranded.share > Num::ONE {
                        return Err(broken(AirfieldError::StrandedShare));
                    }
                    let kind = kinds.and_then(|kinds| kinds.named(stranded.kind.as_str()));
                    let kind = kind.ok_or_else(|| {
                        let name = UnknownName::new(DamageKind::NAMES, &stranded.kind);
                        BookError::unknown(unit.package, at(), name)
                    })?;
                    Ok(StrandedLoss {
                        share: stranded.share,
                        kind,
                    })
                })
                .transpose()?;
            let spec = Self::of(data, stranded, rate);
            if spec.acceleration <= Num::ZERO {
                return Err(broken(AirfieldError::Acceleration));
            }
            book.set(unit.unit_type, spec);
        }
        Ok(book.build())
    }

    /// `data` at `rate`, of its loss `stranded`.
    fn of(data: &SortieData, stranded: Option<StrandedLoss>, rate: MatchRate) -> Self {
        let hz = NonZeroI64::from(rate.hz()).get();
        Self {
            pause: rate
                .ticks(u64::from(data.pause_ms))
                .expect("a pause of a u32 of milliseconds fits the ticks"),
            idle: data.idle_ms.map(|ms| {
                rate.ticks(u64::from(ms))
                    .expect("an idle time of a u32 of milliseconds fits the ticks")
            }),
            acceleration: data
                .acceleration
                .checked_div_int(hz * hz, Rounding::NearestEven)
                .expect("an acceleration over a rate squared fits"),
            taxi_step: data
                .taxi_speed
                .checked_div_int(hz, Rounding::NearestEven)
                .expect("a speed over a rate fits"),
            stranded,
        }
    }
}

impl PerId<UnitType> for SortieSpec {
    const SCRIPTS_ONLY: bool = false;
}

impl DataSection for SortieData {
    const TABLE: DataTable = DataTable::new("A unit type's `sortie`");
}

impl Section for SortieData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "sortie";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
