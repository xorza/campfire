use std::error::Error;
use std::sync::Arc;

use campfire_actions::{CostTarget, SlotKind, SlotKinds};
use campfire_capabilities::{Books, Release};
use campfire_combat::ModeCombatData;
use campfire_declare::{BookError, BookPlace, NamesListError, Section};
use campfire_mode::{ModeList, ModeMap, ModeNames, ModeNamesError};
use campfire_navigation::ModeNavigationData;
use campfire_players::ResourcesData;
use campfire_progression::{ModeTracksData, TrackBook};
use campfire_stats::ModePoolsData;
use campfire_units::UnitTypes;
use campfire_values::{Names, PackageIndex, PathId, Team, TrackId, TypeScope};

use crate::mode_packages::ModePackages;

/// A kind of name that a table of `NameTables` holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TableKind {
    Resource,
    Pool,
    DamageKind,
    Track,
    SlotKind,
    Layer,
    Tag,
    Team,
    Path,
    UnitType,
}

/// The names the mode's packages declare, each kind in the table a match builds of it, by the
/// constructor its book is built by: what the load check resolves a name of data or a script
/// by. So a name past a kind's limit, or a kind's name twice, fails as the book fails, before a
/// check could find the name missing.
#[derive(Debug)]
pub(crate) struct NameTables {
    /// The pools, the players' resources, the damage kinds and the layers.
    mode: ModeNames,
    tracks: Arc<Names<TrackId>>,
    /// The slot kinds' own table, which their data builds as it reads.
    slot_kinds: Arc<Names<SlotKind>>,
    /// The engine's tags, then every tag the packages name; each package's unit types in its
    /// scope, the mode's and the avatars in the mode's.
    types: UnitTypes,
    teams: Names<Team>,
    /// The map's paths, each a name of the first that names it: the map's resolve refuses a
    /// repeat, and a path id counts more paths than a map file holds.
    paths: Names<PathId>,
}

impl NameTables {
    pub(crate) fn of(packages: &ModePackages) -> Result<Self, BookError> {
        let data = &packages.data;
        let book_packages = packages.book_packages();
        let engine_tags = Release::engine_tags();
        let types = Books::unit_types(&engine_tags, packages.tag_names(), &book_packages);
        let tracks = data.section_or_default::<ModeTracksData>();
        let tracks = TrackBook::new(&tracks.0);
        let tracks =
            tracks.map_err(|error| Self::error(BookPlace::Section(ModeTracksData::KEY), error))?;
        let paths = packages.map.paths().iter().map(|path| path.name.as_str());
        Ok(Self {
            mode: ModeNames::of(data).map_err(Self::names_error)?,
            tracks: Arc::clone(tracks.names()),
            slot_kinds: Arc::clone(data.section_or_default::<SlotKinds>().names()),
            types: types?,
            teams: ModeMap::team_names(&packages.manifest.teams)
                .map_err(|error| Self::error(BookPlace::Teams, error))?,
            paths: Names::first_of(paths),
        })
    }

    /// The book error of `error`, a rule the mode's data at `at` breaks.
    fn error<E: Error + Clone + PartialEq + Send + Sync + 'static>(
        at: BookPlace,
        error: E,
    ) -> BookError {
        BookError::rule(PackageIndex::MODE, at, error)
    }

    /// The book error of `error`, a list of the mode's names that does not load.
    fn names_error(error: ModeNamesError) -> BookError {
        let (list, error) = match error {
            ModeNamesError::TooMany(list) => (list, NamesListError::TooMany),
            ModeNamesError::Repeated { list, name } => (list, NamesListError::Repeated(name)),
        };
        let key = match list {
            ModeList::Pools => ModePoolsData::KEY,
            ModeList::Resources => ResourcesData::KEY,
            ModeList::DamageKinds => ModeCombatData::KEY,
            ModeList::Layers => ModeNavigationData::KEY,
        };
        Self::error(BookPlace::Section(key), error)
    }

    /// Whether the mode declares `text` a name of `kind`: a unit type of the mode's scope.
    pub(crate) fn has(&self, kind: TableKind, text: &str) -> bool {
        let mode = &self.mode;
        match kind {
            TableKind::Resource => mode.resources.named(text).is_some(),
            TableKind::Pool => mode.pools.named(text).is_some(),
            TableKind::DamageKind => mode.damage_kinds.named(text).is_some(),
            TableKind::Track => self.tracks.named(text).is_some(),
            TableKind::SlotKind => self.slot_kinds.named(text).is_some(),
            TableKind::Layer => mode.layers.named(text).is_some(),
            TableKind::Tag => self.types.tags().named(text).is_some(),
            TableKind::Team => self.teams.named(text).is_some(),
            TableKind::Path => self.paths.named(text).is_some(),
            TableKind::UnitType => self.types.types().named((TypeScope::Mode, text)).is_some(),
        }
    }

    /// What a cost named `name` takes from: a pool, or else a player resource.
    pub(crate) fn cost_target(&self, name: &str) -> Option<CostTarget> {
        CostTarget::named(&self.mode.pools, &self.mode.resources, name)
    }
}
