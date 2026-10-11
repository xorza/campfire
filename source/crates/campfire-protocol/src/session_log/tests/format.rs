//! A log file is read by the format its tag names: a format that changes under the same
//! `LOG_TAG` would read an older file's bytes as other values. Each tag pins the file of
//! `minimal`, which holds every kind of byte a log file holds, and no tag's file is written again.

use std::env;
use std::path::{Path, PathBuf};

use campfire_store::{DirEntries, DurableFile, EntryKind, InputFile};

use super::*;

/// The most bytes a pinned file may hold: `minimal` takes 3,619.
const MAX_LEN: usize = 1 << 16;

fn pinned_dir() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/session-log"
    ))
}

/// The name of the file that pins `tag`: its last part, as `v3.log`.
fn file_name(tag: &[u8]) -> String {
    let version = tag.rsplit(|&byte| byte == b'/').next().unwrap();
    format!("{}.log", str::from_utf8(version).unwrap())
}

/// Reads each file of `dir` but `current`, and asserts the decode refuses it as no log.
fn assert_older_refused(dir: &Path, current: &str) {
    for entry in DirEntries::read(dir).unwrap() {
        assert_eq!(entry.kind, EntryKind::File, "{:?} is a file", entry.name);
        if entry.name == current {
            continue;
        }
        let bytes = InputFile::read(&dir.join(&entry.name), MAX_LEN).unwrap();
        assert_eq!(
            SessionLog::decode(&bytes).err(),
            Some(LogError::NotLog),
            "{:?}, of another tag, is refused as no log",
            entry.name
        );
    }
}

#[test]
fn a_log_format_changes_only_with_its_tag() {
    let dir = pinned_dir();
    let name = file_name(LOG_TAG);
    let path = dir.join(&name);
    let now = encoded(&minimal());
    match InputFile::read_if_present(&path, MAX_LEN).unwrap() {
        Some(pinned) => assert!(
            pinned == now,
            "the bytes of {name} changed: a change of the format raises `LOG_TAG`; a change of \
             `minimal` alone removes {name}, then writes it again with CAMPFIRE_BLESS=1"
        ),
        None if env::var_os("CAMPFIRE_BLESS").is_some() => DurableFile::write(&path, &now).unwrap(),
        None => panic!("no file pins {name}: write it with CAMPFIRE_BLESS=1"),
    }
    assert_older_refused(&dir, &name);
}

#[test]
fn a_pinned_file_is_named_by_its_tags_version() {
    assert_eq!(file_name(b"campfire/session-log/v3"), "v3.log");
    assert_eq!(file_name(b"campfire/session-log/v12"), "v12.log");
}
