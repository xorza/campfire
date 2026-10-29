use std::collections::BTreeMap;
use std::num::NonZeroU8;

use bevy::asset::Handle;
use bevy::ecs::resource::Resource;
use bevy::image::Image;
use bevy::math::Rect;
use campfire_client_files::{
    GridCell, HudFile, Key, PanelButton, PanelCommand, PanelGrid, PanelIcon,
};
use campfire_names::MatchNames;
use campfire_units::TypeOriginBook;
use campfire_values::{ActionId, NameId, PackageIndex, PackagePath, UnitType};
use tracing::error;

use crate::engine_text::{EngineMessage, EngineText};
use crate::view::client_data::ClientLoad;

/// What the panel draws of its buttons, as the packages' client files give it (02-engine-core.md, A
/// button's look is its action's; 02-engine-core.md, A look places its buttons): the HUD file's
/// grid, the face of each action whose look gives one and of each command, the places of each unit
/// type's look's buttons, by type, none for a look that places none, and the face of each type
/// whose look gives its icon, which a button that stands for its unit shows. A place past the grid,
/// and an action no action of the match is, are logged and left out.
#[derive(Resource, Debug)]
pub(super) struct PanelLooks {
    grid: PanelGrid,
    actions: BTreeMap<ActionId, CellFace>,
    commands: BTreeMap<PanelCommand, CommandFace>,
    places: Vec<Box<[LookPlace]>>,
    units: Vec<Option<CellFace>>,
}

/// A button's face: its icon, none for a button that shows its name, and its key.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct CellFace {
    pub(super) icon: Option<FaceIcon>,
    pub(super) key: Option<Key>,
}

/// An icon: its texture, and its rectangle in the texture's pixels.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct FaceIcon {
    pub(super) image: Handle<Image>,
    pub(super) rect: Rect,
}

/// A command's face, and its name in the engine's own text.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CommandFace {
    pub(super) face: CellFace,
    pub(super) name: String,
}

/// A button a look places: its cell of the grid, and what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LookPlace {
    pub(super) cell: GridCell,
    pub(super) button: PlacedButton,
}

/// What a placed button is: an action of its type's slots, or a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlacedButton {
    Action(ActionId),
    Command(PanelCommand),
}

/// What the panel's looks load from: the match's unit types and names, the packages' client data,
/// the mode's HUD file, and the engine's own text.
#[derive(Debug, Clone, Copy)]
pub(super) struct LookSources<'a> {
    pub(super) origins: &'a TypeOriginBook,
    pub(super) names: &'a MatchNames,
    pub(super) load: &'a ClientLoad,
    pub(super) hud: &'a HudFile,
    pub(super) text: &'a EngineText,
}

impl PanelLooks {
    /// The looks of `sources`, each icon's texture as `image` loads a path of a package.
    pub(super) fn of(
        sources: LookSources<'_>,
        mut image: impl FnMut(PackageIndex, &PackagePath) -> Handle<Image>,
    ) -> Self {
        let LookSources {
            origins,
            names,
            load,
            hud,
            text,
        } = sources;
        let grid = hud.panel.clone();
        let mut face = |package, icon: Option<&PanelIcon>, key| CellFace {
            icon: icon.map(|icon| FaceIcon {
                image: image(package, &icon.texture),
                rect: rect(icon),
            }),
            key,
        };
        let mut actions = BTreeMap::new();
        for (at, package) in (0..=u16::MAX).zip(&load.packages) {
            let package_index = PackageIndex::new(at);
            for (name, look) in package.units.actions() {
                if look.icon.is_none() && look.key.is_none() {
                    continue;
                }
                let Some(action) = names.named::<ActionId>((package_index, name.as_str())) else {
                    continue;
                };
                actions.insert(action, face(package_index, look.icon.as_ref(), look.key));
            }
        }
        let commands = PanelCommand::ALL
            .into_iter()
            .map(|command| {
                let look = grid.commands.get(&command);
                let icon = look.and_then(|look| look.icon.as_ref());
                let key = look.and_then(|look| look.key);
                let command_face = CommandFace {
                    face: face(PackageIndex::MODE, icon, key),
                    name: text.text(EngineMessage::of(command)),
                };
                (command, command_face)
            })
            .collect();
        let places = origins
            .iter()
            .map(|(_, origin)| {
                let look = load.of(origin.package).units.units().get(&*origin.name);
                let places = look.into_iter().flat_map(|look| &look.panel);
                let placed = places.filter_map(|place| {
                    let look = &*origin.name;
                    let Some(cell) = grid.cell(place.place) else {
                        error!(
                            look,
                            place = place.place,
                            "a look's place is past its panel's grid"
                        );
                        return None;
                    };
                    let button = match &place.button {
                        PanelButton::Action(name) => {
                            let action = names.named::<ActionId>((origin.package, name.as_str()));
                            let Some(action) = action else {
                                error!(
                                    look,
                                    action = name.as_str(),
                                    "a look's place names no action"
                                );
                                return None;
                            };
                            PlacedButton::Action(action)
                        }
                        &PanelButton::Command(command) => PlacedButton::Command(command),
                    };
                    Some(LookPlace { cell, button })
                });
                placed.collect()
            })
            .collect();
        let units = origins
            .iter()
            .map(|(_, origin)| {
                let look = load.of(origin.package).units.units().get(&*origin.name);
                let icon = look.and_then(|look| look.icon.as_ref());
                icon.map(|icon| face(origin.package, Some(icon), None))
            })
            .collect();
        Self {
            grid,
            actions,
            commands,
            places,
            units,
        }
    }

    pub(super) const fn grid(&self) -> &PanelGrid {
        &self.grid
    }

    /// The face of `action`'s button, none for an action whose look gives none.
    pub(super) fn action(&self, action: ActionId) -> Option<&CellFace> {
        self.actions.get(&action)
    }

    pub(super) fn command(&self, command: PanelCommand) -> &CommandFace {
        &self.commands[&command]
    }

    /// The face of a button that stands for a unit of `unit_type`, none for a type whose look
    /// gives no icon.
    pub(super) fn unit(&self, unit_type: UnitType) -> Option<&CellFace> {
        self.units.get(unit_type.index())?.as_ref()
    }

    /// The buttons the look of `unit_type` places, none for a look that places none.
    pub(super) fn places(&self, unit_type: UnitType) -> &[LookPlace] {
        self.places
            .get(unit_type.index())
            .map_or(&[], |places| places)
    }

    /// The cell of the button at `slot_order` of a look that places none, as the grid's places
    /// run from 1; none past the grid.
    pub(super) fn cell_in_order(&self, slot_order: usize) -> Option<GridCell> {
        let place = u8::try_from(slot_order + 1).ok().and_then(NonZeroU8::new)?;
        self.grid.cell(place)
    }
}

/// The rectangle of `icon` in its texture's pixels.
fn rect(icon: &PanelIcon) -> Rect {
    let [left, top, right, bottom] = icon.rect.edges().map(f32::from);
    Rect::new(left, top, right, bottom)
}

#[cfg(test)]
pub(super) mod internals {
    use std::collections::BTreeMap;

    use campfire_client_files::{PanelCommand, PanelGrid};
    use campfire_values::ActionId;

    use crate::command_panel::panel_looks::{CellFace, CommandFace, LookPlace, PanelLooks};

    impl PanelLooks {
        /// Looks of `grid`, of the faces `actions`, of commands of no face named by their
        /// names in snake case, and of the places `places`, by type.
        pub(crate) fn new(
            grid: PanelGrid,
            actions: BTreeMap<ActionId, CellFace>,
            places: Vec<Box<[LookPlace]>>,
        ) -> Self {
            let commands = PanelCommand::ALL
                .into_iter()
                .map(|command| {
                    let face = CommandFace {
                        face: CellFace::default(),
                        name: format!("{command:?}"),
                    };
                    (command, face)
                })
                .collect();
            Self {
                grid,
                actions,
                commands,
                places,
                units: Vec::new(),
            }
        }
    }
}
