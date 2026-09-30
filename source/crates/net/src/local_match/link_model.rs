/// How a `LocalMatch` link carries packets, the same each way, in steps of the match, so a run
/// repeats exactly: each packet waits `delay` steps and up to `jitter` more, or is lost, one in
/// every `1000 / loss_per_mille`; the draws follow `seed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkModel {
    pub delay: u32,
    pub jitter: u32,
    pub loss_per_mille: u32,
    pub seed: u64,
}

impl LinkModel {
    /// Every packet arrives in the step it was sent.
    pub const PERFECT: LinkModel = LinkModel {
        delay: 0,
        jitter: 0,
        loss_per_mille: 0,
        seed: 0,
    };
}
