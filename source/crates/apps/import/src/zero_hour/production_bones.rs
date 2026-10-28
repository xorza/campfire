use std::collections::BTreeMap;

use campfire_common::DeclaredName;

use crate::zero_hour::airfield_bones::AirfieldBones;

/// What production reads of the objects' models, by their types' names, on the ground in each
/// object's frame, the game's axes: the helipad of each whose model names its bone, each
/// airfield's places and runways, and the dock point of each whose model names its `DockAction`
/// bone, where a gatherer docks (exits-docks-and-selection.md, A dock has its point).
#[derive(Debug, Clone, Default)]
pub(crate) struct ProductionBones {
    pub(crate) helipads: BTreeMap<DeclaredName, [f32; 2]>,
    pub(crate) airfields: BTreeMap<DeclaredName, AirfieldBones>,
    pub(crate) docks: BTreeMap<DeclaredName, [f32; 2]>,
}
