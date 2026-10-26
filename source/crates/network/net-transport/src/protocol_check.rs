use bevy_ecs::component::Component;

/// Where a link's protocol check stands: this end sent its hashes and waits for the peer's, every
/// other channel held, or the peer's matched and every channel sends (11-net-stack/transport.md,
/// The protocol check goes first on a reserved channel). A link's reset takes it away, and the
/// next packet session checks again.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProtocolCheck {
    Waiting,
    Passed,
}
