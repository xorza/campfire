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
mod tests {
    use super::*;

    #[test]
    fn the_vocabulary_goes_where_the_line_says_or_to_the_target_directory() {
        let line = |args: &[&str]| CommandLine::try_parse_from(args).unwrap();
        let target = Path::new("/t");
        assert_eq!(
            line(&["vocabulary", "/out.txt"]).out_file(target),
            PathBuf::from("/out.txt")
        );
        assert_eq!(
            line(&["vocabulary"]).out_file(target),
            PathBuf::from("/t/vocabulary.txt")
        );
    }
}
