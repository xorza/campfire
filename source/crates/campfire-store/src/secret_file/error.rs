use std::io;

use thiserror::Error;

/// Why a secret file did not read.
#[derive(Debug, Error)]
pub enum SecretReadError {
    #[error("could not read the file")]
    Read(#[source] io::Error),
    /// Others than its owner may read it: the bits of its mode, `0o644` as an example.
    #[error("others may read the file (mode {mode:o}); make it 600")]
    Exposed { mode: u32 },
}
