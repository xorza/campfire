/// An airfield's places and runways on the ground in its object's frame, the game's axes, as
/// `ParkingPlaceBehavior::buildInfo` reads their bones: each place, by row then column, of its
/// `Runway<c>Parking<r>` bone, or its `Runway<c>Park<r>Han` one for an airfield that parks in
/// hangars, and its `Runway<c>Prep<r>`; and each column's runway, of its `RunwayStart<c>` and
/// `RunwayEnd<c>`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AirfieldBones {
    pub(crate) places: Vec<PlaceBones>,
    pub(crate) runways: Vec<[[f32; 2]; 2]>,
}

/// An airfield's place: its point, the angle in degrees its bone faces, from the game's `x`
/// toward its `y`, the point it waits at for its runway, and its column's runway.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlaceBones {
    pub(crate) at: [f32; 2],
    pub(crate) angle: f32,
    pub(crate) prep: [f32; 2],
    pub(crate) runway: u8,
}
