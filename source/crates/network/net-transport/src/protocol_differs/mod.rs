use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// A peer's protocol check gave hashes other than this end's, so its link unlinks: the link's
/// bits, and the registries that differ or that one end lacks, by name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct ProtocolDiffers {
    pub(crate) link: u64,
    pub(crate) differing: String,
}

impl LogEvent for ProtocolDiffers {
    const MESSAGE: &'static str = "unlinking a peer whose protocol differs";

    fn log(&self) {
        warn!(link = self.link, differing = %self.differing, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests;
