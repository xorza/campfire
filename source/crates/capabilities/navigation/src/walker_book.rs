use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, BookCx, BookError, ById};
use campfire_names::MatchNames;
use campfire_units::Variations;
use campfire_values::UnitType;

use crate::walker::Walker;
use crate::walker_kinds::WalkerKinds;

/// The kinds of walker of the match's unit types, which the map's walk check and its pathing grid
/// read: navigation derives them once, from its book of each type's walker.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct WalkerBook {
    pub kinds: WalkerKinds,
}

impl WalkerBook {
    /// The kinds of the walkers of the match's unit types, and of each variation of a type that
    /// walks, as each unit walks by its own body
    /// (04-capabilities/00-overview.md, A unit type may vary its units).
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let walkers = cx.book_if::<ById<UnitType, Walker>>()?;
        let variations = cx.book_if::<ById<UnitType, Variations>>()?;
        let varied = walkers
            .into_iter()
            .flat_map(ById::iter)
            .flat_map(|(unit_type, _)| {
                let of_type = variations.and_then(|variations| variations.get(unit_type));
                let forms = of_type.into_iter().flat_map(Variations::iter);
                forms.map(|&form| {
                    Walker::of_form(Some(form)).expect("a walker's variations are circles")
                })
            });
        let kinds = walkers
            .into_iter()
            .flat_map(ById::values)
            .copied()
            .chain(varied)
            .collect();
        Ok(Self { kinds })
    }
}

impl Book for WalkerBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}
