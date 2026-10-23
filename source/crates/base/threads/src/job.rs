use campfire_common::NamedEnum;

/// A job that holds an OS thread of its own: every thread the engine starts, each under its name,
/// which a profiler and a panic show (02-engine-core.md, Every thread is a job of one list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, NamedEnum)]
pub enum Job {
    /// Writes the process's log file.
    Log,
    /// Appends and syncs a session's journal.
    Journal,
    /// Applies, encodes, hashes and writes a session's checkpoint.
    Checkpoints,
    /// Writes a client's latest receipt.
    Receipts,
    /// Runs a singleplayer client's local server.
    #[name = "local-server"]
    LocalServer,
    /// Waits on a UDP endpoint's socket and runs its packet sessions' timers.
    #[name = "net-endpoint"]
    Endpoint,
    /// Maps a server's port at its router.
    #[name = "port-map"]
    PortMap,
}

impl Job {
    /// The most bytes of a thread's name Linux keeps, `TASK_COMM_LEN` less its NUL.
    pub const MAX_NAME: usize = 15;
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// Each job's name is whole in a profiler, at most 15 bytes, and no two jobs share one.
    #[test]
    fn each_job_has_its_own_name_whole() {
        let names: Vec<&str> = Job::ALL.iter().map(|job| job.name()).collect();
        assert_eq!(names.len(), 7);
        let long: Vec<&str> = (names.iter().copied())
            .filter(|name| name.len() > Job::MAX_NAME)
            .collect();
        assert_eq!(long, Vec::<&str>::new());
        assert_eq!(names.iter().collect::<BTreeSet<_>>().len(), names.len());
    }
}
