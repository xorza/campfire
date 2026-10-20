use crate::job::Job;

/// The name a worker's thread takes: a job's, or behind `internals` a test's own, each whole in a
/// profiler, at most `Job::MAX_NAME` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadName(&'static str);

impl ThreadName {
    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// A test's own thread `name`.
    #[cfg(feature = "internals")]
    pub const fn fixture(name: &'static str) -> Self {
        assert!(name.len() <= Job::MAX_NAME, "a thread's name is whole");
        Self(name)
    }
}

impl From<Job> for ThreadName {
    fn from(job: Job) -> Self {
        Self(job.name())
    }
}
