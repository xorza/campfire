use std::path::{Path, PathBuf};

use clap::Parser;

/// Writes the repository's layout and the names its Rust files declare to one text file.
#[derive(Debug, Parser)]
#[command(version)]
pub(crate) struct CommandLine {
    /// The file the vocabulary goes to, in place of one there; `vocabulary.txt` in the
    /// workspace's target directory when none is given
    pub(crate) out: Option<PathBuf>,
}

impl CommandLine {
    /// The file the vocabulary goes to: the one given, or `vocabulary.txt` in `target`.
    pub(crate) fn out_file(self, target: &Path) -> PathBuf {
        self.out.unwrap_or_else(|| target.join("vocabulary.txt"))
    }
}

#[cfg(test)]
mod tests;
