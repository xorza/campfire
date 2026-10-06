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
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
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
