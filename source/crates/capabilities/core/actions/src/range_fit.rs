/// How a target lies from a unit against an action's reach, from edge to edge: nearer than its
/// minimum range, within its reach, or beyond its range (12-missiles.md, A minimum range).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeFit {
    Near,
    Within,
    Far,
}
