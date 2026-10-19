use bytes::BytesMut;

/// A datagram a session sends, and the path it goes on.
#[derive(Debug)]
pub(crate) struct Transmit {
    pub(crate) path: Path,
    pub(crate) datagram: BytesMut,
}

/// The path of a datagram: the session's peer address, or a candidate it checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Path {
    Peer,
    Candidate,
}
