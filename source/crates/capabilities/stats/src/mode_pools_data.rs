use campfire_script::{ApiVersion, Status};
use campfire_units::PoolId;
use std::collections::BTreeMap;
use std::sync::Arc;

use campfire_common::DeclaredName;
use campfire_declarations::{BookCx, BookError, BookPlace, NamesListError, Section, SectionTable};
use campfire_players::ResourcesData;
use campfire_values::{Names, NamesError, PackageIndex};
use serde::Deserialize;

use crate::pool_data::PoolData;

/// The mode's `[pools]`, by name, the life pool among them: the amounts its units spend and lose.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct ModePoolsData(pub BTreeMap<DeclaredName, PoolData>);

impl Section for ModePoolsData {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "pools";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

impl ModePoolsData {
    /// The rule of the mode's pools and its players' resources: no name is both, as a cost names
    /// one or the other.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let pools = cx.mode_section::<Self>();
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
    pub(crate) fn names(cx: &BookCx<'_>) -> Result<Arc<Names<PoolId>>, BookError> {
        let mut names = Names::default();
        let pools = cx.mode_section::<Self>().into_iter();
        for name in pools.flat_map(|pools| pools.0.keys()) {
            names.try_add(name.as_str()).map_err(|error| {
                let error = match error {
                    NamesError::Full => NamesListError::TooMany,
                    NamesError::Repeated => NamesListError::Repeated(name.clone()),
                };
                BookError::rule(PackageIndex::MODE, BookPlace::Section(Self::KEY), error)
            })?;
        }
        Ok(Arc::new(names))
    }
}
