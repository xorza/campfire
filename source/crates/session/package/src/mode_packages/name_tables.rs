use std::sync::Arc;

use campfire_actions::{CostTarget, SlotKind};
use campfire_capabilities::{BookError, Books};
use campfire_mode::{ModeMap, ModeNames};
use campfire_progression::TrackBook;
use campfire_units::UnitTypeBook;
use campfire_values::{Names, PathId, ResourceId, Team, TrackId, TypeScope};

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
pub(crate) struct NameTables<'a> {
    /// The pools, the players' resources, the damage kinds and the layers.
    mode: ModeNames,
    tracks: Arc<Names<TrackId>>,
    /// The slot kinds' own table, which their data builds as it reads.
    slot_kinds: &'a Names<SlotKind>,
    /// The engine's tags, then every tag the packages name; each package's unit types in its
    /// scope, the mode's and the avatars in the mode's.
    types: UnitTypeBook,
    teams: Names<Team>,
    /// The map's paths, each a name of the first that names it: the map's resolve refuses a
    /// repeat, and a path id counts more paths than a map file holds.
    paths: Names<PathId>,
}

impl<'a> NameTables<'a> {
    pub(crate) fn of(packages: &'a ModePackages) -> Result<NameTables<'a>, BookError> {
        let data = &packages.data;
        let book_packages = packages.book_packages();
        let types = Books::unit_types(packages.tag_names(), &book_packages);
        let tracks = TrackBook::new(&data.tracks).map_err(BookError::Tracks)?;
        let paths = packages.map.paths.iter().map(|path| path.name.as_str());
        Ok(NameTables {
            mode: ModeNames::of(data).map_err(BookError::Names)?,
            tracks: Arc::clone(tracks.names()),
            slot_kinds: data.slots.names(),
            types: types.map_err(BookError::Types)?,
            teams: ModeMap::team_names(&packages.manifest.teams).map_err(BookError::Mode)?,
            paths: Names::first_of(paths),
        })
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

    pub(crate) fn resources(&self) -> &Names<ResourceId> {
        &self.mode.resources
    }

    /// What a cost named `name` takes from: a pool, or else a player resource.
    pub(crate) fn cost_target(&self, name: &str) -> Option<CostTarget> {
        CostTarget::named(&self.mode.pools, &self.mode.resources, name)
    }
}
