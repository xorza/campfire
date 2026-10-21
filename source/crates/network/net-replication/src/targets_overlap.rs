use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// An entity whose prediction and interpolation targets both name a link, which gets `Predicted`
/// alone (11-net-stack/replication.md, A client that both targets name is predicted alone): the
/// entity's and the link's bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) struct TargetsOverlap {
    pub(crate) entity: u64,
    pub(crate) link: u64,
}

impl LogEvent for TargetsOverlap {
    const MESSAGE: &'static str =
        "the prediction and interpolation targets both name a link, which gets Predicted alone";

    fn log(&self) {
        warn!(entity = self.entity, link = self.link, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_as_it_logs() {
        round_trip(&TargetsOverlap {
            entity: 4_294_967_303,
            link: 12,
        });
    }
}
