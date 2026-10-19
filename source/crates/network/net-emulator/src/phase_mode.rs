/// What a `Phase` does with the packets it admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseMode {
    /// Packets follow the phase's model.
    Pass,
    /// Packets follow the phase's model but arrive no earlier than the next phase's start, as a
    /// stalled link releases what it held in one burst; with no next phase, they never arrive.
    Hold,
}
