use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// The server refused the join of `link`, as `Debug` writes the link's entity, as the lobby's
/// other lines name it; `error` says why.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct JoinRefused {
    pub link: String,
    pub error: String,
}

impl LogEvent for JoinRefused {
    const MESSAGE: &'static str = "refused a join";

    fn log(&self) {
        warn!(link = %self.link, error = %self.error, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests;
