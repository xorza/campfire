use campfire_actions::ActionBook;
use campfire_capabilities::{
    Books, GroundLocation, Location, MarkerLocation, NamedRefs, NearLocation,
};
use campfire_common::DeclaredName;
use campfire_geometry::{Bounds, GridSearch};
use campfire_items::ItemBook;
use campfire_math::{Num, Vec3};
use campfire_mode::MarkerSpec;
use campfire_script::{Filter, UnknownName};
use campfire_units::{GroundPoint, MapBook, RefMap, UnitTypeBook};
use campfire_values::{
    ActionId, Article, FilterData, ItemId, NameKind, NameScope, Named as _, TypeScope, UnitType,
};

use crate::bot_script::error::BotScriptError;
use crate::loaded::{ActionName, Loaded, LoadedGround};

/// The kind of a map marker's name, which a bot's script names a location by.
const MARKERS: NameKind = NameKind::new("marker", Article::A, NameScope::Match);

/// What a bot's script names, as the books of its match hold it: the unit types and their tags,
/// the actions, the item types, the map's metric and its markers.
#[derive(Debug, Clone, Copy)]
pub struct BotNames<'b> {
    types: &'b UnitTypeBook,
    actions: &'b ActionBook,
    items: Option<&'b ItemBook>,
    map: &'b MapBook,
    markers: &'b [MarkerSpec],
}

/// Gives each name of a script the reference a bot's order holds, and keeps the action name of
/// the slot of the one action it maps, when it has one.
#[derive(Debug)]
pub(crate) struct LoadRefs<'n, 'b> {
    names: &'n BotNames<'b>,
    pub(crate) slot: Option<ActionName>,
}

impl<'b> BotNames<'b> {
    /// The names of the match of `books`; a match with no items names no item type.
    pub fn new(books: &'b Books) -> Self {
        Self {
            types: books.get().expect("a match has unit types"),
            actions: books.get().expect("a match has an action book"),
            items: books.get(),
            map: books.get().expect("a match has a map"),
            markers: books.markers(),
        }
    }

    /// The unit type `name` names in the mode's scope.
    pub(crate) fn unit_type(&self, name: &DeclaredName) -> Result<UnitType, BotScriptError> {
        let types = self.types.types();
        let found = types.named((TypeScope::Mode, name.as_str()));
        Ok(found.ok_or_else(|| UnknownName::new(UnitType::NAMES, name))?)
    }

    /// The map of a script's names to a bot's references.
    pub(crate) const fn refs(&self) -> LoadRefs<'_, 'b> {
        LoadRefs {
            names: self,
            slot: None,
        }
    }
}

impl RefMap<NamedRefs, Loaded> for LoadRefs<'_, '_> {
    type Error = BotScriptError;

    fn unit(&mut self, unit: FilterData) -> Result<Filter, BotScriptError> {
        Ok(Filter::resolve(&unit, self.names.types.tags())?)
    }

    /// The actions of the name in every package that names one so, as a unit's slots may hold an
    /// action of any of the match's packages.
    fn slot(&mut self, slot: DeclaredName) -> Result<ActionName, BotScriptError> {
        let names = self.names.actions.names();
        let actions: Vec<ActionId> = names
            .iter()
            .filter(|&(_, name)| name == slot.as_str())
            .map(|(action, _)| action)
            .collect();
        if actions.is_empty() {
            return Err(UnknownName::new(ActionId::NAMES, &slot).into());
        }
        let name = ActionName(actions.into());
        debug_assert!(self.slot.is_none(), "an action names one slot");
        self.slot = Some(name.clone());
        Ok(name)
    }

    /// A location's point; a search for a place, which only a build makes, fails.
    fn ground(&mut self, ground: GroundLocation) -> Result<LoadedGround, BotScriptError> {
        match ground {
            GroundLocation::At(at) => Ok(LoadedGround::At(GroundPoint::under(self.point(at)?))),
            GroundLocation::Near(_) => Err(BotScriptError::NearOutsideBuild),
        }
    }

    /// A location's point, or a search round one of a positive gap within the steps a search
    /// reaches.
    fn place(&mut self, place: GroundLocation, _: Num) -> Result<LoadedGround, BotScriptError> {
        let GroundLocation::Near(NearLocation { near, gap, steps }) = place else {
            return self.ground(place);
        };
        if gap <= Num::ZERO {
            return Err(BotScriptError::NearGap(gap));
        }
        if steps > GridSearch::MAX_STEPS {
            return Err(BotScriptError::NearSteps(steps));
        }
        Ok(LoadedGround::Near {
            near: GroundPoint::under(self.point(near)?),
            gap,
            steps,
        })
    }

    fn point(&mut self, point: Location) -> Result<Vec3, BotScriptError> {
        let map = self.names.map;
        match point {
            Location::Marker(MarkerLocation { marker }) => {
                let found = self.names.markers.iter().find(|spec| spec.name == marker);
                let spec = found.ok_or_else(|| UnknownName::new(MARKERS, &marker))?;
                spec.position.ok_or(BotScriptError::NoPoint(marker))
            }
            Location::At(at) => at
                .place(map.metric(), Bounds::WORLD)
                .map_err(BotScriptError::Point),
        }
    }

    fn item(&mut self, item: DeclaredName) -> Result<ItemId, BotScriptError> {
        let found = self
            .names
            .items
            .and_then(|items| items.names().named(item.as_str()));
        Ok(found.ok_or_else(|| UnknownName::new(ItemId::NAMES, &item))?)
    }
}
