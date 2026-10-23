/// Where a queue of mode events answers among the tick's queues, from cause to effect: what a
/// unit lost before what it gained by it, what a module made before where units arrived; queues
/// of one phase answer in the order their modules declare them (06-script-api.md, A queue's
/// module declares it, in a phase).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventPhase {
    /// Units lost, as deaths.
    Losses,
    /// What units gained, as the levels they reached.
    Gains,
    /// What producers made, as units trained and sites completed.
    Makes,
    /// Where units arrived, as the ends of their paths.
    Arrivals,
}
