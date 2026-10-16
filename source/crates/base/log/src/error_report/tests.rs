use std::io;

use super::*;

/// A step that failed for `source`.
#[derive(Debug)]
struct Step {
    name: &'static str,
    source: Option<io::Error>,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

impl Error for Step {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref()?)
    }
}

#[test]
fn a_report_writes_each_error_down_to_the_root() {
    let alone = Step {
        name: "the key file",
        source: None,
    };
    assert_eq!(ErrorReport::of(&alone).to_string(), "the key file");

    // `io::Error::other` writes its inner error's message and gives that error's source, as a
    // transparent variant does, so the inner step shows once.
    let inner = Step {
        name: "not an nsec",
        source: Some(io::Error::new(io::ErrorKind::InvalidData, "bad checksum")),
    };
    let outer = Step {
        name: "the key file",
        source: Some(io::Error::other(inner)),
    };
    assert_eq!(
        ErrorReport::of(&outer).to_string(),
        "the key file: not an nsec: bad checksum"
    );
}
