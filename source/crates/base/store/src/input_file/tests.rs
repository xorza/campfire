use std::fs::OpenOptions;
use std::io::Write;

use super::*;
use crate::dir_entries::DirEntries;
use crate::test_dir::TestDir;

#[test]
fn a_read_gives_a_files_bytes_within_its_bound_and_says_why_it_refuses() {
    let test_dir = TestDir::new();
    test_dir.write("five", b"12345");
    let five = test_dir.path("five");
    let refused = |path: &Path, max_len| InputFile::read(path, max_len).unwrap_err().error;
    // Its bytes at the bound, or within; one byte past it is too large, by its length or by
    // what it reads; the error names the path.
    assert_eq!(InputFile::read(&five, 5).unwrap(), b"12345");
    assert_eq!(InputFile::read(&five, 64).unwrap(), b"12345");
    assert!(matches!(refused(&five, 4), ReadError::TooLarge { max: 4 }));
    assert_eq!(InputFile::read(&five, 4).unwrap_err().path, five);
    assert_eq!(InputFile::read_text(&five, 5).unwrap(), "12345");
    // A stream within the bound gives the bytes and the file's length as it opened; one byte
    // past is too large before it reads.
    let mut streamed = Vec::new();
    let mut stream = InputFile::stream(&five, 5).unwrap();
    assert_eq!(stream.stamp().len, 5);
    stream.read_to_end(&mut streamed).unwrap();
    assert_eq!(streamed, b"12345");
    assert!(matches!(
        InputFile::stream(&five, 4).unwrap_err().error,
        ReadError::TooLarge { max: 4 }
    ));

    let none = test_dir.path("none");
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
    test_dir.create_dir("dir");
    assert!(matches!(
        refused(&test_dir.path("dir"), 64),
        ReadError::NotFile
    ));
    // A pipe is no file, and opens without waiting for a writer, so one swapped in after its
    // path's check is still refused at once.
    if test_dir.pipe("pipe") {
        let pipe = test_dir.path("pipe");
        assert!(matches!(refused(&pipe, 64), ReadError::NotFile));
        let opened = NoFollow::open(&pipe).unwrap().unwrap();
        assert!(!opened.metadata().unwrap().is_file());
    }
    // A link to the file is refused, by each read, and leads to the file by its target's path;
    // one to no file leads nowhere.
    test_dir.link("five", "link");
    let link = test_dir.path("link");
    assert!(matches!(refused(&link, 64), ReadError::Link));
    assert!(matches!(
        InputFile::ranges(&link).unwrap_err().error,
        ReadError::Link
    ));
    let target = DirEntries::link_target(&link).unwrap();
    assert_eq!(target, DirEntries::link_target(&five).unwrap());
    assert_eq!(InputFile::read(&target, 5).unwrap(), b"12345");
    test_dir.link("none", "dangling");
    assert!(matches!(
        DirEntries::link_target(&test_dir.path("dangling"))
            .unwrap_err()
            .error,
        ReadError::Missing
    ));
    assert!(matches!(
        refused(&test_dir.path("dangling"), 64),
        ReadError::Link
    ));
    assert!(matches!(
        refused(&test_dir.path("bad\0name"), 64),
        ReadError::Read(_)
    ));

    // Bytes that are not UTF-8 are no text, and read as bytes.
    test_dir.write("binary", [0xff, 0xfe]);
    let binary = test_dir.path("binary");
    assert!(matches!(
        InputFile::read_text(&binary, 64).unwrap_err().error,
        ReadError::NotText(_)
    ));
    assert_eq!(InputFile::read(&binary, 64).unwrap(), [0xff, 0xfe]);

    // A stream gives the length it opened with and the bytes; it refuses as a read does.
    let mut stream = InputFile::stream(&five, 64).unwrap();
    assert_eq!(stream.len(), 5);
    let mut streamed = Vec::new();
    stream.read_to_end(&mut streamed).unwrap();
    assert_eq!(streamed, b"12345");
    assert!(matches!(
        InputFile::stream(&test_dir.path("dir"), 64)
            .unwrap_err()
            .error,
        ReadError::NotFile
    ));

    // Ranges of "12345" from one handle: from the start, from the middle, to the end, and none
    // at the end; a range past the 5 bytes it opened with is short, and names the path, as one
    // whose end passes what a u64 holds is.
    let ranges = InputFile::ranges(&five).unwrap();
    assert_eq!(ranges.len(), 5);
    assert_eq!(ranges.read_at(0, 2).unwrap(), b"12");
    assert_eq!(ranges.read_at(1, 3).unwrap(), b"234");
    assert_eq!(ranges.read_at(2, 3).unwrap(), b"345");
    assert_eq!(ranges.read_at(5, 0).unwrap(), b"");
    let short = ranges.read_at(4, 2).unwrap_err();
    assert!(matches!(
        short.error,
        ReadError::Short { offset: 4, len: 2 }
    ));
    assert_eq!(short.path, five);
    // A length far past the file is short before its buffer is made, which would not fit memory.
    assert!(matches!(
        ranges.read_at(0, usize::MAX).unwrap_err().error,
        ReadError::Short {
            offset: 0,
            len: usize::MAX
        }
    ));
    assert!(matches!(
        ranges.read_at(u64::MAX, 1).unwrap_err().error,
        ReadError::Short {
            offset: u64::MAX,
            len: 1
        }
    ));
    assert!(matches!(
        InputFile::ranges(&test_dir.path("dir")).unwrap_err().error,
        ReadError::NotFile
    ));
    assert!(matches!(
        InputFile::ranges(&none).unwrap_err().error,
        ReadError::Missing
    ));

    // The time of change is its handle's, no later than now; a stream's stamp, the ranges' and
    // the file's own hold it and the length; a missing file has none.
    let stamp = InputFile::stream(&five, 64).unwrap().stamp();
    assert!(stamp.modified <= SystemTime::now());
    assert_eq!(stamp.len, 5);
    assert_eq!(ranges.stamp(), stamp);
    assert_eq!(InputFile::stamp(&five).unwrap(), stamp);
    assert!(matches!(
        InputFile::stamp(&none).unwrap_err().error,
        ReadError::Missing
    ));
}

#[test]
fn a_file_that_grows_past_its_bound_as_it_is_read_is_refused() {
    // The metadata gives 3 bytes, within a bound of 4; the file then holds 6, of which the read
    // takes 5, one past the bound, and refuses them.
    let test_dir = TestDir::new();
    test_dir.write("growing", b"abc");
    let opened = InputFile::open(&test_dir.path("growing")).unwrap();
    OpenOptions::new()
        .append(true)
        .open(test_dir.path("growing"))
        .unwrap()
        .write_all(b"def")
        .unwrap();
    assert!(matches!(
        opened.read(4),
        Err(ReadError::TooLarge { max: 4 })
    ));
}

#[test]
fn a_range_the_file_no_longer_holds_as_it_is_read_is_short() {
    // The handle opened 6 bytes; the file then holds 3, so the 4 bytes from byte 1 end after 2,
    // and the 3 from byte 0 are still whole.
    let test_dir = TestDir::new();
    test_dir.write("shrinking", b"abcdef");
    let ranges = InputFile::ranges(&test_dir.path("shrinking")).unwrap();
    assert!(ranges.unchanged().unwrap());
    OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(test_dir.path("shrinking"))
        .unwrap()
        .write_all(b"abc")
        .unwrap();
    assert_eq!(ranges.len(), 6);
    // Its handle sees the write: the file is no longer the one it opened.
    assert!(!ranges.unchanged().unwrap());
    assert!(matches!(
        ranges.read_at(1, 4).unwrap_err().error,
        ReadError::Short { offset: 1, len: 4 }
    ));
    assert_eq!(ranges.read_at(0, 3).unwrap(), b"abc");
    // The file opened again has another stamp: its length is 3.
    assert_eq!(
        InputFile::ranges(&test_dir.path("shrinking"))
            .unwrap()
            .stamp()
            .len,
        3
    );
    assert_ne!(
        InputFile::ranges(&test_dir.path("shrinking"))
            .unwrap()
            .stamp(),
        ranges.stamp()
    );
}
