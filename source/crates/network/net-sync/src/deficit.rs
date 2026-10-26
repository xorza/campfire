/// How a timeline closes an error that puts it behind its objective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deficit {
    /// It runs faster until it meets its objective, as it closes a lead, once the error passes
    /// its deadband.
    Slew,
    /// It steps to its objective at once, at any error, as NTP steps an offset it may not slew,
    /// so its deadband lies ahead of its objective alone (11-net-stack/time.md, A client behind
    /// steps, a client ahead slews; 11-net-stack/time.md, The input timeline's band lies ahead of
    /// its objective).
    Step,
}
