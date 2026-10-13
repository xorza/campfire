use std::env;
use std::ffi::OsStr;
use std::path::Path;

use crate::durable_file::DurableFile;
use crate::durable_file::error::DurableError;
use crate::path_error::PathError;

/// A file a test pins: it holds what the test makes, and a run with `CAMPFIRE_BLESS=1` writes it
/// again. Each pinning test compares the file by its own rule, and blesses it by this one.
#[derive(Debug, Clone, Copy)]
pub struct PinnedFile<'a> {
    path: &'a Path,
}

impl<'a> PinnedFile<'a> {
    /// The variable whose value `1` writes every pinned file again.
    const BLESS: &'static str = "CAMPFIRE_BLESS";

    pub const fn new(path: &'a Path) -> PinnedFile<'a> {
        PinnedFile { path }
    }

    /// Writes `now` into the file when the run blesses, and gives whether it did.
    pub fn bless(&self, now: &[u8]) -> Result<bool, PathError<DurableError>> {
        self.write_if(PinnedFile::blessing(), now)
    }

    /// Whether the run writes the pinned files again, by `CAMPFIRE_BLESS`.
    fn blessing() -> bool {
        let value = env::var_os(PinnedFile::BLESS);
        let value = match value.as_deref().map(OsStr::to_str) {
            None => BlessValue::Unset,
            Some(Some(text)) => BlessValue::Text(text),
            Some(None) => BlessValue::NotText,
        };
        PinnedFile::blessing_of(value)
    }

    /// Whether `value` of `CAMPFIRE_BLESS` blesses: `1` does; none, the empty text and `0` pin.
    /// Any other value is a mistake of the run, which no test should take for either.
    fn blessing_of(value: BlessValue<'_>) -> bool {
        match value {
            BlessValue::Unset | BlessValue::Text("" | "0") => false,
            BlessValue::Text("1") => true,
            BlessValue::Text(text) => panic!(
                "{}={text:?}: 1 writes the pinned files again, 0 or none pins them",
                PinnedFile::BLESS
            ),
            BlessValue::NotText => panic!(
                "{} holds bytes that are no text: 1 writes the pinned files again, 0 or none pins \
                 them",
                PinnedFile::BLESS
            ),
        }
    }

    fn write_if(&self, blessing: bool, now: &[u8]) -> Result<bool, PathError<DurableError>> {
        if blessing {
            DurableFile::write(self.path, now)?;
        }
        Ok(blessing)
    }
}

/// What `CAMPFIRE_BLESS` holds: no value, a text, or bytes that are no text.
#[derive(Debug, Clone, Copy)]
enum BlessValue<'a> {
    Unset,
    Text(&'a str),
    NotText,
}

#[cfg(test)]
mod tests {
    use std::panic;

    use super::*;
    use crate::input_file::InputFile;
    use crate::scratch::Scratch;

    #[test]
    fn only_one_blesses_and_a_value_of_neither_meaning_panics() {
        let blessing = |value: &str| PinnedFile::blessing_of(BlessValue::Text(value));
        assert!(!PinnedFile::blessing_of(BlessValue::Unset));
        assert!(!blessing(""));
        assert!(!blessing("0"));
        assert!(blessing("1"));
        for value in ["yes", "true", "2", " 1"] {
            assert!(
                panic::catch_unwind(|| blessing(value)).is_err(),
                "{value:?}"
            );
        }
        assert!(panic::catch_unwind(|| PinnedFile::blessing_of(BlessValue::NotText)).is_err());
    }

    #[test]
    fn a_pinned_file_is_written_only_when_the_run_blesses() {
        let scratch = Scratch::new();
        let path = scratch.path("pinned.txt");
        let file = PinnedFile::new(&path);
        assert!(!file.write_if(false, b"made").unwrap());
        assert_eq!(InputFile::read_if_present(&path, 16).unwrap(), None);
        assert!(file.write_if(true, b"made").unwrap());
        assert_eq!(
            InputFile::read_if_present(&path, 16).unwrap().as_deref(),
            Some(&b"made"[..])
        );
    }
}
