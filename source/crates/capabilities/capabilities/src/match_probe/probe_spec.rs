use std::num::{NonZeroU32, NonZeroU64, ParseIntError};
use std::str::FromStr;

use campfire_common::{DeclaredName, PlayerSlot, StableNumber};
use campfire_sim::StableId;
use campfire_units::UnitTypeBook;
use campfire_values::{Team, TypeScope};

use crate::match_probe::error::{ProbeRefusal, ProbeSpecError};
use crate::match_probe::{MatchProbe, ProbeState, ProbeUnits};

/// A probe as text names it, `<period_ms>:<units>`, as `CAMPFIRE_PROBE`, the Zero Hour check's
/// `--probe` and the verifier's take it: how often it writes, and the units it prints, `all`,
/// `player=<n>` counted from 1, as a check names its players, `team=<n>`, `type=<name>`,
/// `ids=<id>,<id>` or `state=<state>`, as `stuck` names the walkers that make no progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeSpec {
    pub every_ms: NonZeroU64,
    pub units: ProbeUnitsSpec,
}

/// The units a probe's text names, a type by its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeUnitsSpec {
    All,
    Owner(PlayerSlot),
    Team(Team),
    Type(DeclaredName),
    Ids(Vec<StableId>),
    State(ProbeState),
}

impl ProbeSpec {
    /// Its probe of a match of `types` at `hz` ticks a second; a refusal for a period of no
    /// whole count of ticks, which a probe would round, or a type the match has not.
    pub fn probe(
        &self,
        types: Option<&UnitTypeBook>,
        hz: NonZeroU32,
    ) -> Result<MatchProbe, ProbeRefusal> {
        let scaled = self.every_ms.checked_mul(NonZeroU64::from(hz));
        let every = scaled
            .filter(|scaled| scaled.get() % 1000 == 0)
            .and_then(|scaled| NonZeroU64::new(scaled.get() / 1000))
            .ok_or(ProbeRefusal::Period(self.every_ms))?;
        let units = match &self.units {
            ProbeUnitsSpec::All => ProbeUnits::All,
            &ProbeUnitsSpec::Owner(slot) => ProbeUnits::Owner(slot),
            &ProbeUnitsSpec::Team(team) => ProbeUnits::Team(team),
            ProbeUnitsSpec::Type(name) => {
                let found =
                    types.and_then(|types| types.types().named((TypeScope::Mode, name.as_str())));
                ProbeUnits::Type(found.ok_or_else(|| ProbeRefusal::NoType(name.clone()))?)
            }
            ProbeUnitsSpec::Ids(ids) => ProbeUnits::Ids(ids.clone()),
            &ProbeUnitsSpec::State(state) => ProbeUnits::State(state),
        };
        Ok(MatchProbe::new(every, units))
    }
}

impl FromStr for ProbeSpec {
    type Err = ProbeSpecError;

    fn from_str(text: &str) -> Result<Self, ProbeSpecError> {
        let (every_ms, units) = text.split_once(':').ok_or(ProbeSpecError::Shape)?;
        Ok(Self {
            every_ms: every_ms.parse().map_err(ProbeSpecError::Period)?,
            units: units.parse()?,
        })
    }
}

impl FromStr for ProbeUnitsSpec {
    type Err = ProbeSpecError;

    fn from_str(text: &str) -> Result<Self, ProbeSpecError> {
        if text == "all" {
            return Ok(Self::All);
        }
        let (kind, value) = text.split_once('=').ok_or(ProbeSpecError::Units)?;
        Ok(match kind {
            "player" => {
                let player: u32 = number(value)?;
                let index = player.checked_sub(1).ok_or(ProbeSpecError::Player)?;
                Self::Owner(PlayerSlot::new(index))
            }
            "team" => Self::Team(Team::new(number(value)?)),
            "type" => Self::Type(DeclaredName::new(value).ok_or(ProbeSpecError::Name)?),
            "ids" => {
                let ids: Result<Vec<u64>, _> = value.split(',').map(number).collect();
                let ids = ids?.into_iter();
                Self::Ids(
                    ids.map(|id| StableId::from(StableNumber::new(id)))
                        .collect(),
                )
            }
            "state" => Self::State(match value {
                "idle" => ProbeState::Idle,
                "walking" => ProbeState::Walking,
                "stuck" => ProbeState::Stuck,
                "working" => ProbeState::Working,
                "fighting" => ProbeState::Fighting,
                "acting" => ProbeState::Acting,
                _ => return Err(ProbeSpecError::State),
            }),
            _ => return Err(ProbeSpecError::Units),
        })
    }
}

/// The number `text` names.
fn number<T: FromStr<Err = ParseIntError>>(text: &str) -> Result<T, ProbeSpecError> {
    text.parse().map_err(ProbeSpecError::Number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_probe_reads_its_period_and_its_units() {
        let every = |ms| NonZeroU64::new(ms).unwrap();
        let id = |id| StableId::from(StableNumber::new(id));
        let name = |text| DeclaredName::new(text).unwrap();
        for (text, units) in [
            ("1000:all", ProbeUnitsSpec::All),
            // Players count from 1, as the check names them: player 2 is slot 1.
            ("1000:player=2", ProbeUnitsSpec::Owner(PlayerSlot::new(1))),
            ("1000:team=3", ProbeUnitsSpec::Team(Team::new(3))),
            ("1000:type=dozer", ProbeUnitsSpec::Type(name("dozer"))),
            ("1000:ids=9,2", ProbeUnitsSpec::Ids(vec![id(9), id(2)])),
            ("1000:state=stuck", ProbeUnitsSpec::State(ProbeState::Stuck)),
            ("1000:state=idle", ProbeUnitsSpec::State(ProbeState::Idle)),
        ] {
            let expected = ProbeSpec {
                every_ms: every(1000),
                units,
            };
            assert_eq!(text.parse(), Ok(expected), "{text}");
        }
        for (text, error) in [
            ("1000", ProbeSpecError::Shape),
            ("1000:some", ProbeSpecError::Units),
            ("1000:player=0", ProbeSpecError::Player),
            ("1000:type=", ProbeSpecError::Name),
            ("1000:state=x", ProbeSpecError::State),
        ] {
            assert_eq!(text.parse::<ProbeSpec>(), Err(error), "{text}");
        }
        for text in [
            "0:all",
            "soon:all",
            "1000:team=x",
            "1000:ids=1,,2",
            "1000:player=-1",
        ] {
            let refused = text.parse::<ProbeSpec>();
            let number = matches!(
                refused,
                Err(ProbeSpecError::Period(_) | ProbeSpecError::Number(_))
            );
            assert!(number, "{text}: {refused:?}");
        }
    }
}
