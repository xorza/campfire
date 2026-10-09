use std::fs::OpenOptions;
use std::io::Write;

use super::*;
use crate::scratch::Scratch;

#[test]
fn a_read_gives_a_files_bytes_within_its_bound_and_says_why_it_refuses() {
    let scratch = Scratch::new();
    scratch.write("five", b"12345");
    let five = scratch.path("five");
    let refused = |path: &Path, max_len| InputFile::read(path, max_len).unwrap_err().error;
    // Its bytes at the bound, or within; one byte past it is too large, by its length or by
    // what it reads; the error names the path.
    assert_eq!(InputFile::read(&five, 5).unwrap(), b"12345");
    assert_eq!(InputFile::read(&five, 64).unwrap(), b"12345");
    assert!(matches!(refused(&five, 4), ReadError::TooLarge { max: 4 }));
    assert_eq!(InputFile::read(&five, 4).unwrap_err().path, five);
    assert_eq!(InputFile::read_text(&five, 5).unwrap(), "12345");

    let none = scratch.path("none");
    assert!(matches!(refused(&none, 64), ReadError::Missing));
    assert_eq!(InputFile::read_if_present(&none, 64).unwrap(), None);
    assert_eq!(
        InputFile::read_if_present(&five, 64).unwrap().as_deref(),
        Some(&b"12345"[..])
    );
    assert!(matches!(
        InputFile::read_if_present(&five, 4).unwrap_err().error,
        ReadError::TooLarge { max: 4 }
    ));
    scratch.create_dir("dir");
    assert!(matches!(
        refused(&scratch.path("dir"), 64),
        ReadError::NotFile
    ));
    assert!(matches!(
        refused(&scratch.path("bad\0name"), 64),
        ReadError::Read(_)
    ));

    // Bytes that are not UTF-8 are no text, and read as bytes.
    scratch.write("binary", [0xff, 0xfe]);
    let binary = scratch.path("binary");
    assert!(matches!(
        InputFile::read_text(&binary, 64).unwrap_err().error,
        ReadError::NotText(_)
    ));
    assert_eq!(InputFile::read(&binary, 64).unwrap(), [0xff, 0xfe]);

    // A stream gives the length it opened with and the bytes; it refuses as a read does.
    let mut stream = InputFile::stream(&five).unwrap();
    assert_eq!(stream.len(), 5);
    let mut streamed = Vec::new();
    stream.read_to_end(&mut streamed).unwrap();
    assert_eq!(streamed, b"12345");
    assert!(matches!(
        InputFile::stream(&scratch.path("dir")).unwrap_err().error,
        ReadError::NotFile
    ));

    // The time of change is its handle's, no later than now.
    let stamped = InputFile::read_stamped(&five, 5).unwrap();
    assert_eq!(stamped.bytes, b"12345");
    assert!(stamped.modified <= SystemTime::now());
}

#[test]
fn a_file_that_grows_past_its_bound_as_it_is_read_is_refused() {
    // The metadata gives 3 bytes, within a bound of 4; the file then holds 6, of which the read
    // takes 5, one past the bound, and refuses them.
    let scratch = Scratch::new();
    scratch.write("growing", b"abc");
    let opened = InputFile::open(&scratch.path("growing")).unwrap();
    OpenOptions::new()
        .append(true)
        .open(scratch.path("growing"))
        .unwrap()
        .write_all(b"def")
        .unwrap();
    assert!(matches!(
        opened.read(4),
        Err(ReadError::TooLarge { max: 4 })
    ));
}
