/// Whether a test of a box counts its boundary as the box's: closed, a body or a path that
/// touches it meets it; open, only one that shares a point of its inside does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Closure {
    Closed,
    Open,
}

impl Closure {
    /// The largest magnitude of a whole value that lies on a box whose magnitudes reach `limit`:
    /// the limit, closed; the limit less one, open, as `|value| < limit` is
    /// `|value| ≤ limit − 1` for a whole value.
    pub(super) const fn bound(self, limit: u128) -> u128 {
        match self {
            Closure::Closed => limit,
            Closure::Open => limit.wrapping_sub(1),
        }
    }
}
