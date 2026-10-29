/// The priorities of the client's replication markers, each a stack's own, in one order: the
/// higher marker wins an update that more than one applies to. Pre-spawn is the strongest, as a
/// locally predicted absence must not be written by a lower marker's first write; a sent
/// interpolated or predicted entity's comes next, before an entity predicted alone.
#[derive(Debug)]
pub struct MarkerPriority;

impl MarkerPriority {
    pub const PRE_SPAWNED: usize = 120;
    pub const PREDICTED_SEND: usize = 100;
    pub const INTERPOLATED: usize = 100;
    pub const PREDICTED: usize = 90;
}
