#![expect(
    unsafe_code,
    reason = "the call for the process's effective user, which std does not give"
)]

use std::fs::{self, DirBuilder, File, Metadata, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::Path;

/// The Unix side of the platform layer: a mode given as a file or directory is made keeps it its
/// owner's only, and `fsync` of a directory keeps its names. A file is its owner's only when its
/// owner is the process's effective user, or root, who may open any file, and its mode lets
/// neither its group nor others in.
#[derive(Debug)]
pub(crate) struct Os;

/// The bits of a mode that let a file's group or others in.
const SHARED: u32 = 0o077;

/// Root's user id.
const ROOT: u32 = 0;

impl Os {
    pub(crate) fn create_owner_only(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
    }

    pub(crate) fn open_owner_only(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)
    }

    pub(crate) fn create_dir_owner_only(path: &Path) -> io::Result<()> {
        DirBuilder::new().mode(0o700).create(path)
    }

    pub(crate) fn exposure(file: &File) -> io::Result<Option<String>> {
        Ok(Os::exposure_of(&file.metadata()?))
    }

    /// Who besides the process's user may open a file of `metadata`.
    fn exposure_of(metadata: &Metadata) -> Option<String> {
        // SAFETY: `geteuid` reads the process's effective user, and has no failure.
        let user = unsafe { libc::geteuid() };
        Os::exposed(metadata.mode(), metadata.uid(), user)
    }

    /// Who besides `user` may open a file of `mode` whose owner is `owner`: the group and others
    /// its mode lets in, and an owner who is neither `user` nor root.
    fn exposed(mode: u32, owner: u32, user: u32) -> Option<String> {
        let mut others = Vec::new();
        if mode & SHARED != 0 {
            others.push(format!("mode {:o}", mode & 0o777));
        }
        if owner != user && owner != ROOT {
            others.push(format!("its owner, user {owner}"));
        }
        (!others.is_empty()).then(|| others.join(", "))
    }

    pub(crate) fn rename(from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    /// `link` fails with `EEXIST` when a file holds `to`, as POSIX makes it; the temporary name
    /// goes once the new one holds the file.
    pub(crate) fn create_name(from: &Path, to: &Path) -> io::Result<()> {
        fs::hard_link(from, to)?;
        fs::remove_file(from)
    }

    pub(crate) fn open_dir(path: &Path) -> io::Result<File> {
        File::open(path)
    }

    pub(crate) fn sync_dir(directory: &Path) -> io::Result<()> {
        File::open(directory)?.sync_all()
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::fs;
    use std::io;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::Path;

    use crate::platform::os::Os;

    impl Os {
        pub(crate) fn expose(path: &Path) -> io::Result<()> {
            fs::set_permissions(path, fs::Permissions::from_mode(0o644))
        }

        pub(crate) fn link_file(target: &Path, link: &Path) -> io::Result<()> {
            symlink(target, link)
        }
    }
}

#[cfg(test)]
pub(crate) mod test_access {
    use std::fs;
    use std::io;
    use std::path::Path;

    use crate::platform::os::Os;

    impl Os {
        pub(crate) fn exposure_at(path: &Path) -> io::Result<Option<String>> {
            Ok(Os::exposure_of(&fs::metadata(path)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::platform::os::Os;

    #[test]
    fn a_file_is_exposed_by_its_mode_and_by_another_owner() {
        // `SHARED`, 0o077, holds the group's and the others' bits; the owner's and the file
        // type's, as a regular file's 0o100000, expose nothing. The user is 1000.
        for (mode, owner) in [
            (0o600, 1000),
            (0o700, 1000),
            (0o400, 1000),
            (0o100_600, 1000),
        ] {
            assert_eq!(Os::exposed(mode, owner, 1000), None, "{mode:o}");
        }
        // Root, who may open any file, owns it: as its own.
        assert_eq!(Os::exposed(0o600, 0, 1000), None);
        for (mode, owner, text) in [
            (0o644, 1000, "mode 644"),
            (0o640, 1000, "mode 640"),
            (0o604, 1000, "mode 604"),
            (0o610, 1000, "mode 610"),
            (0o100_660, 1000, "mode 660"),
            (0o600, 1001, "its owner, user 1001"),
            (0o640, 1001, "mode 640, its owner, user 1001"),
        ] {
            assert_eq!(
                Os::exposed(mode, owner, 1000).as_deref(),
                Some(text),
                "{mode:o}"
            );
        }
        // Root reads a file another user owns as that user's: the promise holds for root too.
        assert_eq!(
            Os::exposed(0o600, 1000, 0).as_deref(),
            Some("its owner, user 1000")
        );
    }
}
