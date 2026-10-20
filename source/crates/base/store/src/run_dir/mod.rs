use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::dir_entries::{DirEntries, EntryKind};
use crate::durable_dir::DurableDir;
use crate::run_dir::error::RunDirError;

pub(crate) mod error;

/// The directory of one run of a check, below its run root, named for the run's start in UTC, to
/// the millisecond: `2026-09-30T17-45-12.123Z`, so the names sort as the starts do. A run touches
/// another run's directory only to remove it, once the root holds more runs than it keeps.
#[derive(Debug)]
pub struct RunDir {
    path: PathBuf,
}

impl RunDir {
    /// The template of a run's name: `0` a digit, any other character itself.
    const TEMPLATE: &str = "0000-00-00T00-00-00.000Z";

    /// Creates the directory of a run that started at `start` below `root`, and `root` if it
    /// does not exist, then keeps `keep` runs, its own and the newest others, and removes the
    /// rest, as pytest keeps its newest `tmp_path` directories, `tmp_path_retention_count`; an
    /// entry of the root that is no run's directory stays. An error if the directory exists, or
    /// another run's does not go.
    pub fn create(root: &Path, start: SystemTime, keep: NonZeroUsize) -> Result<Self, RunDirError> {
        let name = Self::name(start);
        let path = root.join(&name);
        DurableDir::create_all(root).map_err(RunDirError::Root)?;
        DurableDir::create_new(&path).map_err(RunDirError::Dir)?;
        let mut runs: Vec<_> = DirEntries::read(root)
            .map_err(RunDirError::List)?
            .into_iter()
            .filter(|entry| entry.kind == EntryKind::Dir && entry.name != name.as_str())
            .filter(|entry| entry.name.to_str().is_some_and(Self::is_run_name))
            .map(|entry| entry.name)
            .collect();
        runs.sort_unstable();
        let old = runs.len().saturating_sub(keep.get() - 1);
        for name in &runs[..old] {
            DurableDir::remove(&root.join(name)).map_err(RunDirError::Prune)?;
        }
        Ok(Self { path })
    }

    /// Whether `name` is a run's, of `TEMPLATE`'s form.
    fn is_run_name(name: &str) -> bool {
        name.len() == Self::TEMPLATE.len()
            && name
                .bytes()
                .zip(Self::TEMPLATE.bytes())
                .all(|(held, form)| match form {
                    b'0' => held.is_ascii_digit(),
                    form => held == form,
                })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn name(start: SystemTime) -> String {
        let since = start
            .duration_since(UNIX_EPOCH)
            .expect("the clock is after 1970");
        let seconds = since.as_secs();
        let [year, month, day] = Self::civil(seconds / 86_400);
        let time = seconds % 86_400;
        format!(
            "{year:04}-{month:02}-{day:02}T{:02}-{:02}-{:02}.{:03}Z",
            time / 3600,
            time / 60 % 60,
            time % 60,
            since.subsec_millis()
        )
    }

    /// The year, month and day of `days` after 1970-01-01, in the proleptic Gregorian calendar:
    /// days count in 400-year eras of 146 097 days, and each year from March, so February's leap
    /// day ends it.
    const fn civil(days: u64) -> [u64; 3] {
        let from_march = days + 719_468;
        let era = from_march / 146_097;
        let of_era = from_march % 146_097;
        let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
        let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_from_march = (5 * of_year + 2) / 153;
        let day = of_year - (153 * month_from_march + 2) / 5 + 1;
        let month = if month_from_march < 10 {
            month_from_march + 3
        } else {
            month_from_march - 9
        };
        let year = era * 400 + year_of_era + if month <= 2 { 1 } else { 0 };
        [year, month, day]
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::durable_dir::error::DurableDirCreateError;
    use crate::path_error::PathError;
    use crate::test_dir::TestDir;

    fn at(millis: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(millis)
    }

    #[test]
    fn each_run_gets_a_directory_of_its_own_named_for_its_start() {
        assert_eq!(RunDir::name(at(0)), "1970-01-01T00-00-00.000Z");
        // 1 790 000 000 s is 20 717 days and 51 200 s: 14 h 13 min 20 s into 2026-09-21. 2026
        // begins 20 454 days after 1970, so the day is its 264th.
        assert_eq!(
            RunDir::name(at(1_790_000_000_123)),
            "2026-09-21T14-13-20.123Z"
        );
        // The leap day of 2000, 11 016 days after 1970, and the last millisecond of that day.
        assert_eq!(
            RunDir::name(at(11_016 * 86_400_000 + 86_399_999)),
            "2000-02-29T23-59-59.999Z"
        );

        // Below a directory that goes when the test ends, passed or failed.
        let test_dir = TestDir::new();
        let root = test_dir.path("runs");
        let three = NonZeroUsize::new(3).unwrap();
        let first = RunDir::create(&root, at(1_000), three).unwrap();
        let kept = Path::new("runs")
            .join(RunDir::name(at(1_000)))
            .join("server.jsonl");
        test_dir.write(&kept, "kept");
        let second = RunDir::create(&root, at(2_000), three).unwrap();
        assert_ne!(first.path(), second.path());
        assert_eq!(test_dir.read_text(&kept), "kept");
        assert!(matches!(
            RunDir::create(&root, at(1_000), three),
            Err(RunDirError::Dir(PathError {
                error: DurableDirCreateError::Exists,
                ..
            }))
        ));

        // Kept 3, the fourth run removes the first, its file with it, and the fifth the second;
        // a file, and a directory of another name, stay.
        test_dir.write("runs/notes.txt", "notes");
        test_dir.create_dir("runs/1970-01-01T00-00-00.000");
        RunDir::create(&root, at(3_000), three).unwrap();
        RunDir::create(&root, at(4_000), three).unwrap();
        let names = |test_dir: &TestDir| test_dir.names("runs");
        assert_eq!(
            names(&test_dir),
            [
                "1970-01-01T00-00-00.000",
                "1970-01-01T00-00-02.000Z",
                "1970-01-01T00-00-03.000Z",
                "1970-01-01T00-00-04.000Z",
                "notes.txt",
            ]
        );
        RunDir::create(&root, at(5_000), three).unwrap();
        assert_eq!(
            names(&test_dir)[1..4],
            [
                "1970-01-01T00-00-03.000Z",
                "1970-01-01T00-00-04.000Z",
                "1970-01-01T00-00-05.000Z",
            ]
        );
        // One kept is the run's own alone.
        RunDir::create(&root, at(6_000), NonZeroUsize::MIN).unwrap();
        assert_eq!(
            names(&test_dir),
            [
                "1970-01-01T00-00-00.000",
                "1970-01-01T00-00-06.000Z",
                "notes.txt",
            ]
        );
        // A run that starts before the others, as after the clock went back, keeps its own.
        RunDir::create(&root, at(500), NonZeroUsize::MIN).unwrap();
        assert_eq!(
            names(&test_dir),
            [
                "1970-01-01T00-00-00.000",
                "1970-01-01T00-00-00.500Z",
                "notes.txt",
            ]
        );
    }
}
