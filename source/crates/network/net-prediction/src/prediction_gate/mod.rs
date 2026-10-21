//! Whether this app predicts: what the prediction systems run on.
use campfire_net_connection::NetworkingState;

/// Whether this app predicts: it has a prediction setup and is a client. What the prediction
/// systems run on, and what a prediction despawn disables on.
pub(crate) const fn predicts(has_setup: bool, networking: &NetworkingState) -> bool {
    has_setup && networking.topology.is_client()
}

#[cfg(test)]
mod tests;
