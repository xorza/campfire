use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, BookCx, BookError, BookPlace, Section};
use campfire_geometry::Radius;
use campfire_names::MatchNames;
use campfire_values::PackageIndex;

use crate::error::NavigationDataError;
use crate::landing_data::LandingData;
use crate::mode_navigation_data::ModeNavigationData;

/// The mode's landing search, as `LandingData` gives it, its ring checked to be positive.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[component(immutable)]
pub(crate) struct LandingBook(pub(crate) LandingData);

impl LandingBook {
    /// The mode's landing search; the default for a load of none. An error for a ring of no gap.
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let data = cx
            .mode_section::<ModeNavigationData>()
            .map_or_else(LandingData::default, |data| data.landing);
        if data.ring.is_some_and(|ring| ring == Radius::ZERO) {
            let at = BookPlace::Section(ModeNavigationData::KEY);
            return Err(BookError::rule(
                PackageIndex::MODE,
                at,
                NavigationDataError::LandingRing,
            ));
        }
        Ok(Self(data))
    }
}

impl Book for LandingBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}
