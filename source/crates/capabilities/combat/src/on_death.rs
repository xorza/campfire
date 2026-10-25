use std::num::NonZeroU32;

use campfire_declarations::PerId;
use campfire_values::UnitType;
use serde::{Deserialize, Serialize};

/// What becomes of a unit when it dies, as its unit type says: in data, `stay`, `despawn` or
/// `{ keep_ms = <ms> }`. A fact of the type, which a unit's death reads through its type, as its
/// armor is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "OnDeathFile", into = "OnDeathFile")]
pub enum OnDeath {
    /// It stays, dead, for the mode to respawn, as avatars do.
    Stay,
    /// It despawns, as creeps and towers do.
    #[default]
    Despawn,
    /// It stays, dead, for `ms` milliseconds, then despawns, so every client draws its death, as
    /// Zero Hour's `SlowDeathBehavior` keeps its object for its `DestructionDelay`
    /// (animated-looks.md, A death keeps its unit for its animation).
    Keep { ms: NonZeroU32 },
}

/// An `OnDeath` as a file writes it: by its name, or as a time it keeps its unit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(untagged)]
enum OnDeathFile {
    Named(NamedDeath),
    Kept { keep_ms: NonZeroU32 },
}

/// The deaths a file names.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum NamedDeath {
    Stay,
    Despawn,
}

impl PerId<UnitType> for OnDeath {
    const SCRIPTS_ONLY: bool = false;
}

impl From<OnDeathFile> for OnDeath {
    fn from(file: OnDeathFile) -> Self {
        match file {
            OnDeathFile::Named(NamedDeath::Stay) => Self::Stay,
            OnDeathFile::Named(NamedDeath::Despawn) => Self::Despawn,
            OnDeathFile::Kept { keep_ms } => Self::Keep { ms: keep_ms },
        }
    }
}

impl From<OnDeath> for OnDeathFile {
    fn from(death: OnDeath) -> Self {
        match death {
            OnDeath::Stay => Self::Named(NamedDeath::Stay),
            OnDeath::Despawn => Self::Named(NamedDeath::Despawn),
            OnDeath::Keep { ms } => Self::Kept { keep_ms: ms },
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct File {
        on_death: OnDeath,
    }

    #[test]
    fn a_death_reads_by_its_name_or_as_the_time_it_keeps_its_unit_and_writes_back() {
        let read = |text: &str| Toml::parse::<File>(text).map(|file| file.on_death);
        let keep = OnDeath::Keep {
            ms: NonZeroU32::new(2500).unwrap(),
        };
        for (text, death) in [
            (r#"on_death = "stay""#, OnDeath::Stay),
            (r#"on_death = "despawn""#, OnDeath::Despawn),
            ("on_death = { keep_ms = 2500 }", keep),
        ] {
            assert_eq!(read(text).unwrap(), death, "{text}");
            let written = Toml::write(&File { on_death: death });
            assert_eq!(read(&written).unwrap(), death, "{written}");
        }
        // A keep of no time, a name of no death and a field of none fail to read.
        for wrong in [
            "on_death = { keep_ms = 0 }",
            r#"on_death = "linger""#,
            "on_death = { keep_s = 2 }",
        ] {
            assert!(read(wrong).is_err(), "{wrong}");
        }
    }
}
