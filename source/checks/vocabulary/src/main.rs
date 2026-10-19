//! The vocabulary of the repository, on request, for a review of its names: the layout of every
//! file git tracks or would track, each Rust file's items under it with their fields, variants
//! and methods, then an index of every name those files declare by kind, and the count of each
//! word of those names, in one text file whose order is fixed, so two runs compare by a diff.
//! Each Rust file parses with `syn`, so a file that does not parse fails the run after the text
//! is written with the file marked.
//!
//! Run it with `cargo drive run vocabulary [out]`; it writes to `target/vocabulary.txt` unless
//! `out` names another file.

use std::env;
use std::path::Path;
use std::process::ExitCode;

use campfire_common::{ErrorReport, ExitStatus};
use campfire_log::Logging;
use campfire_store::{DurableFile, InputFile};
use tracing::{error, info};

use crate::command_line::CommandLine;
use crate::error::VocabularyError;
use crate::repo_file::RepoFile;
use crate::vocabulary::Vocabulary;

mod command_line;
mod error;
mod index;
mod outline;
mod repo_file;
mod token_text;
mod vocabulary;

/// The most bytes it reads of a Rust file: far past any the repository holds.
const MAX_LEN: usize = 1 << 20;

fn main() -> ExitCode {
    let _log = Logging {
        terminal: "info",
        file: "info",
    }
    .start();
    let line = match Logging::command_line::<CommandLine>(env::args_os()) {
        Ok(line) => line,
        Err(status) => return ExitCode::from(status),
    };
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate is two directories under the workspace");
    let root = workspace
        .parent()
        .expect("the workspace is a directory of the repository");
    let out = line.out_file(&workspace.join("target"));
    match run(root, &out) {
        Ok(vocabulary) if vocabulary.unparsed().is_empty() => {
            info!(out = %out.display(), "the vocabulary is written");
            ExitCode::from(ExitStatus::Success)
        }
        Ok(vocabulary) => {
            for unparsed in vocabulary.unparsed() {
                error!(path = unparsed.path, error = %ErrorReport::of(&unparsed.error), "a Rust file does not parse");
            }
            error!(
                out = %out.display(),
                unparsed = vocabulary.unparsed().len(),
                "the vocabulary is written, with each file that does not parse marked"
            );
            ExitCode::from(ExitStatus::Failure)
        }
        Err(problem) => {
            error!(error = %ErrorReport::of(&problem), "the vocabulary is not written");
            ExitCode::from(ExitStatus::Failure)
        }
    }
}

/// Writes the vocabulary of the repository at `root` to `out`.
fn run(root: &Path, out: &Path) -> Result<Vocabulary, VocabularyError> {
    let files = RepoFile::list(root)?;
    let vocabulary = Vocabulary::of(&files, |path| {
        InputFile::read_text(&root.join(path), MAX_LEN).map_err(VocabularyError::Read)
    })?;
    DurableFile::write(out, vocabulary.text().as_bytes()).map_err(VocabularyError::Write)?;
    Ok(vocabulary)
}
