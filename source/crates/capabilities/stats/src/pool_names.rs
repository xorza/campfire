use std::sync::Arc;

use campfire_declare::{BookCx, BookError, BookPlace, Fact, NamesListError, Section};
use campfire_players::ResourcesData;
use campfire_values::{Names, NamesError, PackageIndex};

use crate::mode_pools_data::ModePoolsData;
use crate::pool_id::PoolId;

/// The names of the mode's pools, by pool.
#[derive(Debug, Clone, Default)]
pub struct PoolNames(pub Arc<Names<PoolId>>);

impl Fact for PoolNames {}

impl PoolNames {
    /// The rule of the mode's pools and its players' resources: no name is both, as a cost names
    /// one or the other.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let pools = cx.mode_section::<ModePoolsData>();
        let resources = cx.mode_section::<ResourcesData>().into_iter();
        let mut resources = resources.flat_map(|resources| &resources.0);
        let both = resources.find(|name| pools.is_some_and(|pools| pools.0.contains_key(*name)));
        both.map_or(Ok(()), |name| {
            let at = BookPlace::Section(ResourcesData::KEY);
            let problem = NamesListError::Repeated(name.clone());
            Err(BookError::rule(PackageIndex::MODE, at, problem))
        })
    }

    /// The names of the mode's `[pools]`; an error past the most pools a match holds.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let mut names = Names::default();
        let pools = cx.mode_section::<ModePoolsData>().into_iter();
        for name in pools.flat_map(|pools| pools.0.keys()) {
            names.try_add(name.as_str()).map_err(|error| {
                let error = match error {
                    NamesError::Full => NamesListError::TooMany,
                    NamesError::Repeated => NamesListError::Repeated(name.clone()),
                };
                BookError::rule(
                    PackageIndex::MODE,
                    BookPlace::Section(ModePoolsData::KEY),
                    error,
                )
            })?;
        }
        Ok(Self(Arc::new(names)))
    }
}
