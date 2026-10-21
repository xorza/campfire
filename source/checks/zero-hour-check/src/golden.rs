//! What the check measured, which `golden.toml` beside it pins and a bless writes again.

use std::path::Path;

use campfire_codec::Toml;
use campfire_store::InputFile;
use campfire_store::internals::PinnedFile;
use serde::{Deserialize, Serialize};

use crate::error::CheckError;
use crate::pins::{
    BotBase, DefenseTicks, Fingerprints, FoughtCounts, LookCounts, MapCounts, MissileTicks,
    ProducedCounts, SkirmishTicks, SuppliedCounts, SupplyTicks, TerrainCounts,
};

/// What the check measured of the known version and this importer, each a value no closed form
/// gives: the fingerprints, a run on Linux and a run on macOS alike; Tournament Desert's counts;
/// the import's looks, trains, builds, supply and weapons; the plays' ticks, the supply play's
/// deliveries, the defense play's hit and kill and the missile play's shot, hit and kill among
/// them; and the bot's base.
/// A run pins them, and a run that blesses writes them again (09-zero-hour.md, The check's
/// measures are a golden file).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Golden {
    /// The ticks player 1's dozer takes from its start to player 2's.
    pub(crate) drive_ticks: u64,
    pub(crate) fingerprints: Fingerprints,
    pub(crate) map: MapCounts,
    pub(crate) terrain: TerrainCounts,
    pub(crate) looks: LookCounts,
    pub(crate) produced: ProducedCounts,
    pub(crate) supplied: SuppliedCounts,
    pub(crate) fought: FoughtCounts,
    pub(crate) skirmish: SkirmishTicks,
    pub(crate) supply: SupplyTicks,
    pub(crate) defense: DefenseTicks,
    pub(crate) missile: MissileTicks,
    pub(crate) bot: BotBase,
}

/// What a run measured so far of `Golden`'s values, each none until its check or its play
/// measured it.
#[derive(Debug, Default)]
pub(crate) struct Measured {
    pub(crate) drive_ticks: Option<u64>,
    pub(crate) fingerprints: Option<Fingerprints>,
    pub(crate) map: Option<MapCounts>,
    pub(crate) terrain: Option<TerrainCounts>,
    pub(crate) looks: Option<LookCounts>,
    pub(crate) produced: Option<ProducedCounts>,
    pub(crate) supplied: Option<SuppliedCounts>,
    pub(crate) fought: Option<FoughtCounts>,
    pub(crate) skirmish: Option<SkirmishTicks>,
    pub(crate) supply: Option<SupplyTicks>,
    pub(crate) defense: Option<DefenseTicks>,
    pub(crate) missile: Option<MissileTicks>,
    pub(crate) bot: Option<BotBase>,
}

impl Golden {
    /// The most bytes the file may hold.
    const MAX_LEN: usize = 1 << 16;

    /// What the file says first: what it is, and how it is written again.
    const HEADER: &'static str = "# What the Zero Hour check measured, which it pins: \
                                  `CAMPFIRE_BLESS=1 cargo drive run zero-hour` writes it \
                                  again.\n\n";

    /// The file beside the check's manifest.
    fn path() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/golden.toml"))
    }

    /// The pinned values, as the file holds them.
    pub(crate) fn read() -> Result<Self, CheckError> {
        let bytes = InputFile::read(Self::path(), Self::MAX_LEN).map_err(CheckError::GoldenRead)?;
        let text = String::from_utf8(bytes).map_err(|_| CheckError::GoldenText)?;
        Toml::parse(&text).map_err(CheckError::GoldenParse)
    }

    /// Writes the values into the file, in place of what it held.
    pub(crate) fn bless(&self) -> Result<(), CheckError> {
        let text = format!("{}{}", Self::HEADER, Toml::write(self));
        let file = PinnedFile::new(Self::path());
        let blessed = file
            .bless(text.as_bytes())
            .map_err(CheckError::GoldenWrite)?;
        debug_assert!(blessed, "the check blesses only in a run that blesses");
        Ok(())
    }

    /// Whether the run writes the file again, in place of pinning its values.
    pub(crate) fn is_blessing() -> bool {
        PinnedFile::is_blessing()
    }
}

impl Measured {
    /// The values every check and play measured; none when one of them did not.
    pub(crate) fn into_golden(self) -> Option<Golden> {
        Some(Golden {
            drive_ticks: self.drive_ticks?,
            fingerprints: self.fingerprints?,
            map: self.map?,
            terrain: self.terrain?,
            looks: self.looks?,
            produced: self.produced?,
            supplied: self.supplied?,
            fought: self.fought?,
            skirmish: self.skirmish?,
            supply: self.supply?,
            defense: self.defense?,
            missile: self.missile?,
            bot: self.bot?,
        })
    }
}
