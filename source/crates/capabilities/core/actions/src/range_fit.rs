/// How a target lies from a unit against an action's reach, from edge to edge: nearer than its
/// minimum range, within its reach, or beyond its range (04-capabilities/actions.md, An action may
/// keep a minimum range).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeFit {
    Near,
    Within,
    Far,
}
