use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU8;

use campfire_client_files::{
    ButtonLook, IconRect, Key, PanelButton, PanelCommand, PanelGrid, PanelIcon, PanelPlace,
    PlaceOrder,
};
use campfire_common::DeclaredName;

use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::dropped::Losses;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::csf::string_table::StringTable;
use crate::zero_hour::format::ini::command_button_ini::{ButtonCommand, CommandButton};
use crate::zero_hour::format::ini::mapped_image_ini::MappedImageIni;
use crate::zero_hour::game_path;
use crate::zero_hour::panel_drop_cause::PanelDropCause;
use crate::zero_hour::type_name::TypeName;

/// The buttons of the install's command sets as its producers' panels place them
/// (02-engine-core.md, A look places its buttons): each producer's places, the face of each action
/// and command a button gives, the first button's, and what it drops of them, counted.
#[derive(Debug, Default)]
pub(crate) struct PanelButtons {
    pub(crate) places: BTreeMap<TypeName, Vec<PanelPlace>>,
    pub(crate) actions: BTreeMap<DeclaredName, ButtonFace>,
    pub(crate) commands: BTreeMap<PanelCommand, ButtonFace>,
    pub(crate) dropped: Losses<PanelDropCause>,
}

/// What a button shows: its name, for what it loses, its mapped image's name, and its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ButtonFace {
    pub(crate) button: String,
    pub(crate) image: Option<String>,
    pub(crate) key: Option<Key>,
}

/// The panel the client draws of the buttons: each action's icon and key, the grid, its commands'
/// icons and keys, and the atlases its icons draw from, each once, to convert.
#[derive(Debug, Default)]
pub(crate) struct PanelImport {
    pub(crate) actions: BTreeMap<DeclaredName, ButtonLook>,
    pub(crate) grid: PanelGrid,
    pub(crate) textures: BTreeSet<ArchivePath>,
}

impl PanelButtons {
    /// The last place the game's control bar draws, as `ControlBar.wnd` lays 14 windows; places
    /// 15 to 18 are its scripts' (`MAX_COMMANDS_PER_SET`).
    pub(crate) const PLACES: u8 = 14;

    /// Places at `place` of `producer`'s panel the button `name`, `button`, which made `action`,
    /// none for a button the import does not convert, its key of its label in `strings`; a command
    /// of an order the engine holds places it; another is dropped, as is a place past the last.
    pub(crate) fn place(
        &mut self,
        strings: &StringTable,
        producer: &TypeName,
        (place, name): (u8, &str),
        button: &CommandButton,
        action: Option<DeclaredName>,
    ) {
        let command = match button.command {
            ButtonCommand::Sell => Some(PanelCommand::Sell),
            ButtonCommand::Stop => Some(PanelCommand::Stop),
            ButtonCommand::AttackMove => Some(PanelCommand::AttackMove),
            ButtonCommand::SetRallyPoint => Some(PanelCommand::Rally),
            ButtonCommand::UnitBuild
            | ButtonCommand::DozerConstruct
            | ButtonCommand::PlayerUpgrade
            | ButtonCommand::ObjectUpgrade
            | ButtonCommand::Other => None,
        };
        let placed = match (action, command) {
            (Some(action), _) => PanelButton::Action(action),
            (None, Some(command)) => PanelButton::Command(command),
            // A button of an action the import did not make is counted by its cause already.
            (None, None) if button.command != ButtonCommand::Other => return,
            (None, None) => return self.dropped.drop(name, PanelDropCause::Command),
        };
        let place = NonZeroU8::new(place).filter(|place| place.get() <= Self::PLACES);
        let Some(place) = place else {
            return self.dropped.drop(name, PanelDropCause::Place);
        };
        let face = || ButtonFace {
            button: name.to_owned(),
            image: button.image.clone(),
            key: Self::key(strings, button),
        };
        match &placed {
            PanelButton::Action(action) => {
                self.actions.entry(action.clone()).or_insert_with(face);
            }
            PanelButton::Command(command) => {
                self.commands.entry(*command).or_insert_with(face);
            }
        }
        let places = self.places.entry(producer.clone()).or_default();
        places.push(PanelPlace {
            place,
            button: placed,
        });
    }

    /// The key of `button`'s label in `strings`: the character after its first `&`, a letter or a
    /// digit, as `HotKeyManager::searchHotKey` finds it; none for a label of no mark.
    fn key(strings: &StringTable, button: &CommandButton) -> Option<Key> {
        let text = strings.get(button.label.as_deref()?)?;
        let marked = text.chars().skip_while(|&char| char != '&').nth(1)?;
        let mut cap = [0; 4];
        let cap = marked.to_ascii_uppercase().encode_utf8(&mut cap);
        Key::ALL.iter().copied().find(|key| key.label() == cap)
    }
}

impl PanelImport {
    /// The panel of `buttons`, each image of `images` an icon of its texture, which `texture`
    /// finds in the archives; a button of no image, of a turned one, of no texture or of no key
    /// counted into `dropped`, its icon or its key left out. Its grid is the game's, 7 columns of
    /// 2, its places down each column, as `ControlBar.wnd` lays them.
    pub(crate) fn of(
        buttons: &PanelButtons,
        images: &MappedImageIni,
        texture: impl Fn(&str) -> Option<ArchivePath>,
        dropped: &mut Losses<PanelDropCause>,
    ) -> Result<Self, ZeroHourError> {
        let mut panel = Self {
            grid: PanelGrid {
                columns: NonZeroU8::new(7).expect("7 columns"),
                rows: NonZeroU8::new(2).expect("2 rows"),
                order: PlaceOrder::Columns,
                commands: BTreeMap::new(),
            },
            ..Self::default()
        };
        let mut look = |face: &ButtonFace| -> Result<ButtonLook, ZeroHourError> {
            if face.key.is_none() {
                dropped.drop(&face.button, PanelDropCause::NoKey);
            }
            let image = face.image.as_deref().and_then(|name| images.get(name));
            let found = match image {
                None => Err(PanelDropCause::NoImage),
                Some(image) if image.rotated => Err(PanelDropCause::Rotated),
                Some(image) => match (texture(&image.texture), Self::rect(image.coords)) {
                    (None, _) => Err(PanelDropCause::NoTexture),
                    (Some(_), None) => Err(PanelDropCause::NoImage),
                    (Some(path), Some(rect)) => Ok((path, rect)),
                },
            };
            let icon = match found {
                Ok((path, rect)) => {
                    let icon = PanelIcon {
                        texture: game_path::texture(&path)?,
                        rect,
                    };
                    panel.textures.insert(path);
                    Some(icon)
                }
                Err(cause) => {
                    dropped.drop(&face.button, cause);
                    None
                }
            };
            Ok(ButtonLook {
                icon,
                key: face.key,
            })
        };
        let mut actions = BTreeMap::new();
        for (action, face) in &buttons.actions {
            actions.insert(action.clone(), look(face)?);
        }
        let mut commands = BTreeMap::new();
        for (&command, face) in &buttons.commands {
            commands.insert(command, look(face)?);
        }
        panel.actions = actions;
        panel.grid.commands = commands;
        Ok(panel)
    }

    /// The rectangle of an image's `Coords`, its left, top, right and bottom; none of no pixel or
    /// past what a rectangle holds.
    fn rect([left, top, right, bottom]: [i32; 4]) -> Option<IconRect> {
        let edge = |edge: i32| u16::try_from(edge).ok();
        IconRect::new(edge(left)?, edge(top)?, edge(right)?, edge(bottom)?)
    }
}

#[cfg(test)]
mod tests;
