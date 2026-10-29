/// A visual value's error, which a correction measures as a rollback moves the value and then
/// adds back over its window, decaying: `Delta` is the value itself or a smaller type of its
/// changes. Replication's own diffs are Replicon's `Diffable`.
pub trait VisualDiff<Delta = Self>: Clone {
    /// The difference from `self` to `new` (`new - self`).
    fn diff(&self, new: &Self) -> Delta;

    /// `delta` added to `self` (`self + delta`).
    fn apply_diff(&mut self, delta: &Delta);
}
