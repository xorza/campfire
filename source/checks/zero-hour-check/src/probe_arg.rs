use std::num::{NonZeroU32, NonZeroU64};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use campfire_capabilities::Books;
use campfire_common::{DeclaredName, PlayerSlot, StableNumber};
use campfire_harness::{MatchProbe, ProbeUnits};
use campfire_sim::StableId;
use campfire_units::UnitTypeBook;
use campfire_values::{Team, TypeScope};
use clap::ValueEnum;

use crate::error::ProbeArgError;
use crate::failure::CheckFailure;
use crate::play::Play;

/// A probe the command line asks of a play, `<play>:<period_ms>:<units>`: the play, how often it
/// writes, and the units it prints (debug-loop.md, One probe of a match).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeArg {
    pub(crate) play: Play,
    pub(crate) every_ms: NonZeroU64,
    pub(crate) units: ProbeUnitsArg,
}

/// The units a probe prints, as the command line names them: `all`, `slot=<n>` from 1, as the
/// check names its players, `team=<n>`, `type=<name>` or `ids=<id>,<id>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProbeUnitsArg {
    All,
    Owner(PlayerSlot),
    Team(Team),
    Type(DeclaredName),
    Ids(Vec<StableId>),
}

/// A probe of one play: what it asks, and the file it writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlayProbe {
    pub(crate) every_ms: NonZeroU64,
    pub(crate) units: ProbeUnitsArg,
    pub(crate) file: PathBuf,
}

/// A play's probe as its match takes it, and the file it writes as the play ends.
#[derive(Debug)]
pub(crate) struct ProbeFile {
    pub(crate) probe: MatchProbe,
    pub(crate) file: PathBuf,
}

impl ProbeArg {
    /// The probe of the play it names, writing `<play>.probe` in `dir`.
    pub(crate) fn of_play(&self, dir: &Path) -> PlayProbe {
        PlayProbe {
            every_ms: self.every_ms,
            units: self.units.clone(),
            file: dir.join(format!("{}.probe", self.play.name())),
        }
    }
}

impl PlayProbe {
    /// The probe in a match of `books` at `rate` ticks a second, before the play runs: a failure
    /// for a period of no whole count of ticks, which a probe would round, or for a type the
    /// match has not.
    pub(crate) fn resolve(
        self,
        books: &Books,
        rate: NonZeroU32,
    ) -> Result<ProbeFile, CheckFailure> {
        let scaled = self.every_ms.checked_mul(NonZeroU64::from(rate));
        let every = scaled
            .filter(|scaled| scaled.get() % 1000 == 0)
            .and_then(|scaled| NonZeroU64::new(scaled.get() / 1000))
            .ok_or(CheckFailure::ProbePeriod(self.every_ms))?;
        Ok(ProbeFile {
            probe: MatchProbe::new(every, self.units.resolve(books)?),
            file: self.file,
        })
    }
}

impl ProbeUnitsArg {
    /// The units it names in a match of `books`; a failure for a type the match has not.
    fn resolve(&self, books: &Books) -> Result<ProbeUnits, CheckFailure> {
        Ok(match self {
            Self::All => ProbeUnits::All,
            &Self::Owner(slot) => ProbeUnits::Owner(slot),
            &Self::Team(team) => ProbeUnits::Team(team),
            Self::Type(name) => {
                let types = books.get::<UnitTypeBook>().expect("a match has unit types");
                let found = types.types().named((TypeScope::Mode, name.as_str()));
                ProbeUnits::Type(found.ok_or_else(|| CheckFailure::ProbeType(name.clone()))?)
            }
            Self::Ids(ids) => ProbeUnits::Ids(ids.clone()),
        })
    }
}

impl FromStr for ProbeArg {
    type Err = ProbeArgError;

    fn from_str(text: &str) -> Result<Self, ProbeArgError> {
        let mut parts = text.splitn(3, ':');
        let (Some(play), Some(every_ms), Some(units)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(ProbeArgError::Shape);
        };
        let play = Play::from_str(play, false).map_err(|_| ProbeArgError::Play)?;
        let every_ms = every_ms.parse().map_err(ProbeArgError::Period)?;
        Ok(Self {
            play,
            every_ms,
            units: units.parse()?,
        })
    }
}

impl FromStr for ProbeUnitsArg {
    type Err = ProbeArgError;

    fn from_str(text: &str) -> Result<Self, ProbeArgError> {
        if text == "all" {
            return Ok(Self::All);
        }
        let (kind, value) = text.split_once('=').ok_or(ProbeArgError::Units)?;
        Ok(match kind {
            "slot" => {
                let player: u32 = value.parse().map_err(ProbeArgError::Number)?;
                let index = player.checked_sub(1).ok_or(ProbeArgError::Player)?;
                Self::Owner(PlayerSlot::new(index))
            }
            "team" => Self::Team(Team::new(value.parse().map_err(ProbeArgError::Number)?)),
            "type" => Self::Type(DeclaredName::new(value).ok_or(ProbeArgError::Name)?),
            "ids" => {
                let ids: Result<Vec<u64>, _> = value.split(',').map(str::parse).collect();
                let ids = ids.map_err(ProbeArgError::Number)?.into_iter();
                Self::Ids(
                    ids.map(|id| StableId::from(StableNumber::new(id)))
                        .collect(),
                )
            }
            _ => return Err(ProbeArgError::Units),
        })
    }
}
