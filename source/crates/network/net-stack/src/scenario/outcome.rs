/// What a scenario run found: every invariant it broke, and a digest of everything it observed,
/// which two runs of one seed share.
#[derive(Debug)]
pub(crate) struct ScenarioOutcome {
    pub(crate) failures: Vec<String>,
    pub(crate) digest: u64,
    /// Packets dropped on the wire, so a test can see that its faults fired.
    pub(crate) dropped: u64,
}
