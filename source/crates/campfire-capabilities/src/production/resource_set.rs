use crate::players::resource_id::ResourceId;
use crate::units::bits256::Bits256;

/// A set of the mode's player resources, one bit each.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ResourceSet(Bits256);

impl FromIterator<ResourceId> for ResourceSet {
    fn from_iter<I: IntoIterator<Item = ResourceId>>(resources: I) -> ResourceSet {
        let bits = resources
            .into_iter()
            .fold(Bits256::NONE, |bits, resource| bits.with(resource.index()));
        ResourceSet(bits)
    }
}

impl<const N: usize> From<[ResourceId; N]> for ResourceSet {
    fn from(resources: [ResourceId; N]) -> ResourceSet {
        resources.into_iter().collect()
    }
}

impl ResourceSet {
    pub(crate) const fn contains(self, resource: ResourceId) -> bool {
        self.0.contains(resource.index())
    }
}
