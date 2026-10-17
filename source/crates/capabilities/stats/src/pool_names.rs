use std::sync::Arc;

use campfire_declare::{BookCx, BookError, BookPlace, Fact, NamesListError, Section};
use campfire_values::{Names, NamesError, PackageIndex};

use crate::mode_pools_data::ModePoolsData;
use crate::pool_id::PoolId;

/// The names of the mode's pools, by pool.
#[derive(Debug, Clone, Default)]
pub struct PoolNames(pub Arc<Names<PoolId>>);

impl Fact for PoolNames {}

impl PoolNames {
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
