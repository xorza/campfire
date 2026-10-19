use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use crate::error::VocabularyError;

/// A file of the repository as git lists it, by its path from the repository's root, its names
/// joined by `/`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RepoFile {
    pub(crate) path: String,
    pub(crate) kind: FileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileKind {
    File,
    /// A symbolic link git tracks, which a read does not follow.
    Link,
    /// A repository inside this one, a submodule or an untracked one, which git lists whole.
    Repository,
}

/// The modes git gives a submodule's entry and a link's.
const SUBMODULE_MODE: &str = "160000";
const LINK_MODE: &str = "120000";

impl RepoFile {
    /// Each file of the repository at `root` that git tracks and the worktree still holds, and
    /// each that git would track, by its path's names in order: a directory's files together,
    /// as `a/x` goes before `a-b/x`.
    pub(crate) fn list(root: &Path) -> Result<Vec<Self>, VocabularyError> {
        let staged = git(root, &["ls-files", "-z", "--stage"])?;
        let deleted = git(root, &["ls-files", "-z", "--deleted"])?;
        let others = git(root, &["ls-files", "-z", "--others", "--exclude-standard"])?;
        Self::parse(&staged, &deleted, &others)
    }

    /// The files of `git ls-files -z`'s outputs: `staged` of `--stage`, `deleted` of
    /// `--deleted`, and `others` of `--others`.
    fn parse(staged: &[u8], deleted: &[u8], others: &[u8]) -> Result<Vec<Self>, VocabularyError> {
        let deleted: Vec<&str> = records(deleted).collect::<Result<_, _>>()?;
        // An entry in conflict has one record for each stage.
        let mut files = BTreeMap::new();
        for record in records(staged) {
            let record = record?;
            let (meta, path) = record
                .split_once('\t')
                .ok_or_else(|| VocabularyError::GitRecord(record.to_owned()))?;
            let mode = meta
                .split(' ')
                .next()
                .ok_or_else(|| VocabularyError::GitRecord(record.to_owned()))?;
            if deleted.contains(&path) {
                continue;
            }
            let kind = match mode {
                SUBMODULE_MODE => FileKind::Repository,
                LINK_MODE => FileKind::Link,
                _ => FileKind::File,
            };
            files.insert(path, kind);
        }
        for path in records(others) {
            let path = path?;
            match path.strip_suffix('/') {
                Some(path) => files.insert(path, FileKind::Repository),
                None => files.insert(path, FileKind::File),
            };
        }
        let mut files: Vec<Self> = files
            .into_iter()
            .map(|(path, kind)| Self {
                path: path.to_owned(),
                kind,
            })
            .collect();
        files.sort_by(|a, b| a.path.split('/').cmp(b.path.split('/')));
        Ok(files)
    }
}

/// The records of a `-z` output, each a path or holding one, which must be UTF-8.
fn records(output: &[u8]) -> impl Iterator<Item = Result<&str, VocabularyError>> {
    output
        .split(|&byte| byte == 0)
        .filter(|record| !record.is_empty())
        .map(|record| {
            str::from_utf8(record)
                .map_err(|_| VocabularyError::NotUtf8(String::from_utf8_lossy(record).into_owned()))
        })
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, VocabularyError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(VocabularyError::GitStart)?;
    if !output.status.success() {
        return Err(VocabularyError::Git {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_files_are_those_git_tracks_and_would_track_by_their_names() {
        let staged = concat!(
            "100644 aa 0\tsource/a-b/x.rs\0",
            "100644 bb 0\tsource/a/x.rs\0",
            "160000 cc 0\tsource/drive\0",
            "120000 ab 0\tsource/link.rs\0",
            "100644 dd 1\tgone.rs\0",
            "100644 ee 1\tboth.rs\0",
            "100644 ff 2\tboth.rs\0",
        );
        let files = RepoFile::parse(staged.as_bytes(), b"gone.rs\0", b"new.md\0nested/\0").unwrap();
        let file = |path: &str, kind| RepoFile {
            path: path.to_owned(),
            kind,
        };
        assert_eq!(
            files,
            [
                file("both.rs", FileKind::File),
                file("nested", FileKind::Repository),
                file("new.md", FileKind::File),
                file("source/a/x.rs", FileKind::File),
                file("source/a-b/x.rs", FileKind::File),
                file("source/drive", FileKind::Repository),
                file("source/link.rs", FileKind::Link),
            ]
        );
        assert!(matches!(
            RepoFile::parse(b"100644 aa 0 a.rs\0", b"", b""),
            Err(VocabularyError::GitRecord(record)) if record == "100644 aa 0 a.rs"
        ));
        assert!(matches!(
            RepoFile::parse(b"", b"", b"a\xffb\0"),
            Err(VocabularyError::NotUtf8(path)) if path == "a\u{fffd}b"
        ));
    }
}
