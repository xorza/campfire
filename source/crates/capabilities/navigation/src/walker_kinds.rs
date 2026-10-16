use std::slice;

use crate::walker::Walker;

/// The kinds of walker of a match: each once, by layer, then by radius, as the pathing grid keeps
/// one clearance for each.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WalkerKinds(Vec<Walker>);

impl WalkerKinds {
    pub fn iter(&self) -> slice::Iter<'_, Walker> {
        self.0.iter()
    }

    pub const fn len(&self) -> usize {
        self.0.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The place of `walker` among the kinds, when it is one.
    pub(crate) fn position(&self, walker: Walker) -> Option<usize> {
        self.0.binary_search(&walker).ok()
    }

    /// The widest kind of each layer, by layer.
    pub(crate) fn widest(&self) -> impl Iterator<Item = Walker> + '_ {
        self.0
            .chunk_by(|a, b| a.layer == b.layer)
            .map(|layer| *layer.last().expect("a chunk is never empty"))
    }
}

impl FromIterator<Walker> for WalkerKinds {
    fn from_iter<I: IntoIterator<Item = Walker>>(walkers: I) -> Self {
        let mut kinds: Vec<Walker> = walkers.into_iter().collect();
        kinds.sort_unstable();
        kinds.dedup();
        Self(kinds)
    }
}

impl<const N: usize> From<[Walker; N]> for WalkerKinds {
    fn from(walkers: [Walker; N]) -> Self {
        walkers.into_iter().collect()
    }
}

impl<'a> IntoIterator for &'a WalkerKinds {
    type Item = &'a Walker;
    type IntoIter = slice::Iter<'a, Walker>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::Num;
    use campfire_units::Layer;

    use super::*;

    #[test]
    fn kinds_hold_each_walker_once_by_layer_then_radius() {
        let walker = |layer, radius| Walker {
            layer: Layer::new(layer),
            radius: Num::int(radius),
        };
        let kinds = WalkerKinds::from([walker(1, 2), walker(0, 3), walker(0, 1), walker(0, 3)]);
        let held: Vec<Walker> = kinds.iter().copied().collect();
        assert_eq!(held, [walker(0, 1), walker(0, 3), walker(1, 2)]);
        assert_eq!(kinds.iter().size_hint(), (3, Some(3)));
        assert_eq!(kinds.position(walker(0, 3)), Some(1));
        assert_eq!(kinds.position(walker(1, 1)), None);
        let widest: Vec<Walker> = kinds.widest().collect();
        assert_eq!(widest, [walker(0, 3), walker(1, 2)]);
    }
}
