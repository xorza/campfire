use std::path::Path;

use campfire_codec::Binary;
use campfire_import::{Imported, ZeroHour};
use campfire_store::{DurableFile, InputFile, ReadError};
use serde::{Deserialize, Serialize};

use crate::error::CheckError;

/// What made the import beside it and what the import gave: the install's version and the
/// importer's output version, so a run on the same install with an importer of the same output
/// and the same rules package keeps the import and writes nothing (09-zero-hour.md, A check keeps
/// an unchanged import).
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ImportStamp {
    install: String,
    importer: u32,
    imported: Imported,
}

impl ImportStamp {
    /// The stamp's file, beside the import's two packages.
    pub(crate) const NAME: &'static str = "import.stamp";

    /// The most bytes the file may hold: far past what an import gives.
    const MAX_LEN: usize = 1 << 22;

    /// What the import in `out` gave, when its stamp names `zero_hour`'s install, this
    /// importer's output and the rules package it writes, which an edit of the rules' files
    /// changes with no change of the output's version; none for no stamp, one of another install,
    /// output or rules, or one an older check wrote, which decodes as none.
    pub(crate) fn kept(out: &Path, zero_hour: &ZeroHour) -> Result<Option<Imported>, CheckError> {
        let bytes = match InputFile::read(&out.join(Self::NAME), Self::MAX_LEN) {
            Ok(bytes) => bytes,
            Err(error) if matches!(error.error, ReadError::Missing) => return Ok(None),
            Err(error) => return Err(CheckError::StampRead(error)),
        };
        let Ok(stamp) = Binary::decode::<Self>(&bytes) else {
            return Ok(None);
        };
        let same = stamp.install == zero_hour.version().name()
            && stamp.importer == ZeroHour::OUTPUT_VERSION
            && stamp.imported.rules == ZeroHour::rules_fingerprint();
        Ok(same.then_some(stamp.imported))
    }

    /// Writes the stamp of `imported`, which `zero_hour` wrote into `out`.
    pub(crate) fn write(
        out: &Path,
        zero_hour: &ZeroHour,
        imported: Imported,
    ) -> Result<Imported, CheckError> {
        let stamp = Self {
            install: zero_hour.version().name().to_owned(),
            importer: ZeroHour::OUTPUT_VERSION,
            imported,
        };
        DurableFile::write(&out.join(Self::NAME), &Binary::encode(&stamp))
            .map_err(CheckError::StampWrite)?;
        Ok(stamp.imported)
    }
}
