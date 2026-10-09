use std::path::PathBuf;

use derive_more::Display;
use thiserror::Error;

/// Why a Zero Hour install is not one the importer reads.
#[derive(Debug, Error)]
pub enum ZeroHourError {
    /// An archive of the install is no archive the game reads.
    #[error("{} is no archive", .archive.display())]
    Archive {
        archive: PathBuf,
        #[source]
        error: ArchiveError,
    },
    /// The install's archives are no version the importer knows: the first archive that differs
    /// from the version they were compared with, and how.
    #[error("no version the importer knows: {archive} {difference}")]
    UnknownVersion {
        archive: String,
        difference: VersionDifference,
    },
    /// No archive of the install holds a file the import reads.
    #[error("no archive holds {0}")]
    NotInArchives(String),
}

/// How an install's archive differs from a version's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub enum VersionDifference {
    /// The install has an archive the version has not.
    #[display("is not in it")]
    Extra,
    /// The install lacks an archive of the version.
    #[display("is missing")]
    Missing,
    /// The archive holds other bytes than the version's.
    #[display("holds other bytes")]
    OtherBytes,
}

/// Why an archive's bytes are no `.big` archive.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArchiveError {
    /// It does not start with `BIGF`.
    #[error("it does not start with BIGF")]
    NotBig,
    /// Its header, or its list of entries, ends past the file or its own length.
    #[error("its header ends past its length")]
    Short,
    /// An entry's bytes lie past the file's end.
    #[error("{0}'s bytes lie past its end")]
    EntryPastEnd(String),
    /// An entry's path is not ASCII.
    #[error("an entry's path is not ASCII")]
    NotAscii,
}

/// Why a `RefPack` stream does not decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RefPackError {
    /// It ends inside its header, a command or its literal bytes.
    #[error("it ends inside a command")]
    Short,
    /// A copy reaches back before the first byte written.
    #[error("a copy reaches before the start")]
    BeforeStart,
    /// It writes more bytes than its header gives.
    #[error("it writes more than its {0} bytes")]
    TooLong(usize),
    /// It ends having written fewer bytes than its header gives.
    #[error("it writes {written} of its {len} bytes")]
    TooShort { written: usize, len: usize },
}
