use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::path::Path;

use crate::durable_file::DurableFile;
use crate::durable_file::error::DurableError;
use crate::input_file::InputFile;
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

    pub const fn new(path: &'a Path) -> Self {
        PinnedFile { path }
    }

    /// Writes `now` into the file when the run blesses and the file holds other bytes, and gives
    /// whether the run blesses.
    pub fn bless(&self, now: &[u8]) -> Result<bool, PathError<DurableError>> {
        self.write_if(PinnedFile::is_blessing(), now)
    }

    /// Writes `now` into the file when the run blesses, and else asserts that the file, of at
    /// most `max` bytes, holds `now` byte for byte: the rule of a file a test compares whole.
    pub fn pin(&self, now: &[u8], max: usize) {
        self.pin_if(PinnedFile::is_blessing(), now, max);
    }

    /// Whether the run writes the pinned files again, by `CAMPFIRE_BLESS`.
    pub fn is_blessing() -> bool {
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

    fn pin_if(&self, blessing: bool, now: &[u8], max: usize) {
        let blessed = self.write_if(blessing, now);
        if blessed.unwrap_or_else(|error| panic!("{error}")) {
            return;
        }
        let pinned = InputFile::read(self.path, max).unwrap_or_else(|error| {
            panic!("{error}; {}=1 writes it", PinnedFile::BLESS);
        });
        assert!(
            pinned == now,
            "{} differs from what the test made {}; {}=1 writes it again, and `git diff` shows the \
             rest",
            self.path.display(),
            Difference::of(&pinned, now),
            PinnedFile::BLESS
        );
    }

    /// A file that already holds `now` is left as it is: a write of the same bytes still moves
    /// its time of change, and cargo builds again each crate that includes the file by that time,
    /// not by its bytes.
    fn write_if(&self, blessing: bool, now: &[u8]) -> Result<bool, PathError<DurableError>> {
        if blessing && !self.holds(now) {
            DurableFile::write(self.path, now)?;
        }
        Ok(blessing)
    }

    /// Whether the file holds `now` byte for byte. A file it cannot read holds it not, and the
    /// write that follows reports what is wrong with its path.
    fn holds(&self, now: &[u8]) -> bool {
        InputFile::read_if_present(self.path, now.len())
            .is_ok_and(|held| held.as_deref() == Some(now))
    }
}

/// Where a pinned file and what a test made first differ, for the panic of a pin: the line, with
/// both texts of it, of two texts; else the byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Difference<'a> {
    Line {
        number: usize,
        pinned: Option<&'a str>,
        made: Option<&'a str>,
    },
    Byte {
        offset: usize,
    },
}

impl<'a> Difference<'a> {
    /// The first difference of `pinned` and `made`, which differ.
    fn of(pinned: &'a [u8], made: &'a [u8]) -> Self {
        if let (Ok(pinned), Ok(made)) = (str::from_utf8(pinned), str::from_utf8(made)) {
            let (mut pinned, mut made) = (pinned.lines(), made.lines());
            for number in 1.. {
                let (left, right) = (pinned.next(), made.next());
                if left != right {
                    return Self::Line {
                        number,
                        pinned: left,
                        made: right,
                    };
                }
            }
        }
        let offset = pinned
            .iter()
            .zip(made)
            .position(|(left, right)| left != right)
            .unwrap_or_else(|| pinned.len().min(made.len()));
        Self::Byte { offset }
    }
}

/// `at line 3: pinned "a", made "b"`, a side past its last line `no line`; or `at byte 7`.
impl fmt::Display for Difference<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let side = |line: Option<&str>| {
            line.map_or_else(|| "no line".to_owned(), |line| format!("{line:?}"))
        };
        match *self {
            Self::Line {
                number,
                pinned,
                made,
            } => write!(
                f,
                "at line {number}: pinned {}, made {}",
                side(pinned),
                side(made)
            ),
            Self::Byte { offset } => write!(f, "at byte {offset}"),
        }
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
    use crate::test_dir::TestDir;

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
        let test_dir = TestDir::new();
        let path = test_dir.path("pinned.txt");
        let file = PinnedFile::new(&path);
        assert!(!file.write_if(false, b"made").unwrap());
        assert_eq!(InputFile::read_if_present(&path, 16).unwrap(), None);
        assert!(file.write_if(true, b"made").unwrap());
        assert_eq!(InputFile::read(&path, 16).unwrap(), b"made");
        let made = InputFile::stream(&path, 64).unwrap().stamp();
        // The same bytes leave the file and its time of change as they are; a prefix of them is
        // other bytes, which the file, longer than the bound of their read, is written with.
        assert!(file.write_if(true, b"made").unwrap());
        assert_eq!(InputFile::stream(&path, 64).unwrap().stamp(), made);
        assert!(file.write_if(true, b"ma").unwrap());
        assert_eq!(test_dir.read("pinned.txt"), b"ma");
        assert!(file.write_if(true, b"made").unwrap());
        assert_ne!(
            InputFile::stream(&path, 64).unwrap().stamp().modified,
            made.modified
        );
        // A pin compares the file whole when the run does not bless, and else writes it.
        file.pin_if(false, b"made", 16);
        assert!(panic::catch_unwind(|| file.pin_if(false, b"other", 16)).is_err());
        assert!(panic::catch_unwind(|| file.pin_if(false, b"made", 2)).is_err());
        file.pin_if(true, b"other", 16);
        file.pin_if(false, b"other", 16);
    }

    #[test]
    fn a_difference_names_its_first_line_or_byte() {
        let line = |number, pinned, made| Difference::Line {
            number,
            pinned,
            made,
        };
        assert_eq!(
            Difference::of(b"a\nb\nc", b"a\nx\nc"),
            line(2, Some("b"), Some("x"))
        );
        assert_eq!(Difference::of(b"a\nb", b"a"), line(2, Some("b"), None));
        assert_eq!(
            Difference::of(&[0, 1, 0xff], &[0, 2]),
            Difference::Byte { offset: 1 }
        );
        assert_eq!(
            Difference::of(&[0xff, 1], &[0xff]),
            Difference::Byte { offset: 1 }
        );
        assert_eq!(
            line(2, Some("b"), None).to_string(),
            "at line 2: pinned \"b\", made no line"
        );
    }
}
