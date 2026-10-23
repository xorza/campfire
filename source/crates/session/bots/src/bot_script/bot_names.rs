use std::sync::Arc;

use campfire_actions::ActionBook;
use campfire_capabilities::{
    Books, GroundLocation, Location, MarkerLocation, NamedRefs, NearLocation,
};
use campfire_common::DeclaredName;
use campfire_geometry::{Bounds, GridSearch};
use campfire_items::ItemBook;
use campfire_math::{Num, Vec3};
use campfire_mode::{InputBook, InputType, MarkerSpec};
use campfire_script::{Filter, UnknownName};
use campfire_units::{GroundPoint, MapBook, RefMap, UnitTypeBook};
use campfire_values::{
    ActionId, Article, FilterData, ItemId, NameKind, NameScope, Named as _, TypeScope, UnitType,
};

use crate::bot_script::error::BotScriptError;
use crate::loaded::{ActionName, Loaded, LoadedPlace};
use crate::order_script::{ScriptedInput, ScriptedValue};

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
    inputs: &'b InputBook,
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
            inputs: books.get().expect("a match has the mode's inputs"),
        }
    }

    /// The mode input `input` sends, checked as the mode declares it: its name one of the mode's
    /// inputs, and its value of the input's type.
    pub(crate) fn input(&self, input: &ScriptedInput) -> Result<(), BotScriptError> {
        let name = &input.name;
        let declared = self
            .inputs
            .input_type(name)
            .ok_or_else(|| BotScriptError::NoInput(name.clone()))?;
        let of_type = matches!(
            (&input.value, declared),
            (ScriptedValue::Text(_), InputType::String)
                | (ScriptedValue::List(_), InputType::StringList)
        );
        if of_type {
            Ok(())
        } else {
            Err(BotScriptError::InputType {
                name: name.clone(),
                declared,
            })
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

impl LoadRefs<'_, '_> {
    /// The filter of `unit`, its names resolved, as every reference to a unit takes it.
    fn filter(&self, unit: &FilterData) -> Result<Filter, BotScriptError> {
        Ok(Filter::resolve(unit, self.names.types.tags())?)
    }
}

impl RefMap<NamedRefs, Loaded> for LoadRefs<'_, '_> {
    type Error = BotScriptError;

    fn unit(&mut self, unit: FilterData) -> Result<Filter, BotScriptError> {
        self.filter(&unit)
    }

    fn attack(&mut self, unit: FilterData) -> Result<Filter, BotScriptError> {
        self.filter(&unit)
    }

    fn aimed(&mut self, unit: FilterData) -> Result<Filter, BotScriptError> {
        self.filter(&unit)
    }

    fn used(&mut self, unit: FilterData) -> Result<Filter, BotScriptError> {
        self.filter(&unit)
    }

    fn shop(&mut self, unit: FilterData) -> Result<Filter, BotScriptError> {
        self.filter(&unit)
    }

    fn site(&mut self, unit: FilterData) -> Result<Filter, BotScriptError> {
        self.filter(&unit)
    }

    /// The actions of the name in every package that names one so, as a unit's slots may hold an
    /// action of any of the match's packages.
    fn slot(&mut self, slot: DeclaredName) -> Result<ActionName, BotScriptError> {
        let actions = self.names.actions.actions_named(slot.as_str());
        let actions = actions.ok_or_else(|| UnknownName::new(ActionId::NAMES, &slot))?;
        let name = ActionName(Arc::clone(actions));
        debug_assert!(self.slot.is_none(), "an action names one slot");
        self.slot = Some(name.clone());
        Ok(name)
    }

    /// A location's point on the ground plane.
    fn ground(&mut self, ground: Location) -> Result<GroundPoint, BotScriptError> {
        Ok(GroundPoint::under(self.point(ground)?))
    }

    /// A location's point, or a search round one of a positive gap within the steps a search
    /// reaches.
    fn place(&mut self, place: GroundLocation, _: Num) -> Result<LoadedPlace, BotScriptError> {
        let NearLocation { near, gap, steps } = match place {
            GroundLocation::At(at) => return Ok(LoadedPlace::At(self.ground(at)?)),
            GroundLocation::Near(near) => near,
        };
        if gap <= Num::ZERO {
            return Err(BotScriptError::NearGap(gap));
        }
        if steps > GridSearch::MAX_STEPS {
            return Err(BotScriptError::NearSteps(steps));
        }
        Ok(LoadedPlace::Near {
            near: self.ground(near)?,
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

    /// A stack by its item type's name.
    fn carried(&mut self, carried: DeclaredName) -> Result<ItemId, BotScriptError> {
        self.item(carried)
    }
}
