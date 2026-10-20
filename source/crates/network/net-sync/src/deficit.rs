/// How a timeline closes an error that puts it behind its objective, past its deadband.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deficit {
    /// It runs faster until it meets its objective, as it closes a lead.
    Slew,
    /// It steps to its objective at once, as NTP steps an offset it may not slew
    /// (11-net-stack/time.md, A client behind steps, a client ahead slews).
    Step,
}
