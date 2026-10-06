use std::path::PathBuf;
use std::{env, process};

use super::*;

/// A directory of its own under the system's temporary directory, removed when dropped.
#[derive(Debug)]
pub(crate) struct ScratchDir(pub(crate) PathBuf);

impl ScratchDir {
    pub(crate) fn new(name: &str) -> ScratchDir {
        let dir = env::temp_dir().join(format!("campfire-{name}-{}", process::id()));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(&dir).unwrap();
        ScratchDir(dir)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.0));
    }
}

/// The permission bits of the file at `path`.
#[cfg(unix)]
pub(crate) fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn a_durable_write_replaces_a_file_whole_and_leaves_no_temporary_file() {
    let dir = ScratchDir::new("durable");
    let path = dir.0.join("state");
    DurableFile::write(&path, b"first, and longer").unwrap();
    DurableFile::write(&path, b"second").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"second");
    let names: Vec<_> = fs::read_dir(&dir.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["state"]);
    #[cfg(unix)]
    assert_eq!(mode(&path), 0o600);

    // A crash left a temporary file, which others may read: the write makes its own, so the file
    // is its owner's only.
    let stale = dir.0.join("state.part");
    fs::write(&stale, b"stale and longer").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&stale, fs::Permissions::from_mode(0o644)).unwrap();
    }
    DurableFile::write(&path, b"third").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"third");
    assert!(!stale.exists());
    #[cfg(unix)]
    assert_eq!(mode(&path), 0o600);

    // A path with no file name, and a directory that is not there.
    assert!(matches!(
        DurableFile::write(Path::new("/"), b""),
        Err(DurableError::NoName)
    ));
    let lost = dir.0.join("missing").join("state");
    assert!(matches!(
        DurableFile::write(&lost, b""),
        Err(DurableError::Create(_))
    ));
}

#[test]
fn a_durable_directory_is_made_once_and_its_owners_only() {
    let dir = ScratchDir::new("durable-dir");
    let path = dir.0.join("sessions");
    DurableFile::create_dir(&path).unwrap();
    DurableFile::write(&path.join("kept"), b"kept").unwrap();
    // Made again, it stays as it is.
    DurableFile::create_dir(&path).unwrap();
    assert_eq!(fs::read(path.join("kept")).unwrap(), b"kept");
    #[cfg(unix)]
    assert_eq!(mode(&path), 0o700);
    // A file in its place is no directory.
    assert!(matches!(
        DurableFile::create_dir(&path.join("kept")),
        Err(DurableError::Create(_))
    ));
}
