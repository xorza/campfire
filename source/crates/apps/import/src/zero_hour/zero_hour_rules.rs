use crate::zero_hour::ZeroHour;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::csf::string_table::StringTable;
use crate::zero_hour::format::ini::armor_ini::ArmorIni;
use crate::zero_hour::format::ini::command_button_ini::CommandButtonIni;
use crate::zero_hour::format::ini::command_set_ini::CommandSetIni;
use crate::zero_hour::format::ini::game_data_ini::GameDataIni;
use crate::zero_hour::format::ini::locomotor_ini::LocomotorIni;
use crate::zero_hour::format::ini::mouse_ini::MouseIni;
use crate::zero_hour::format::ini::multiplayer_ini::MultiplayerIni;
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::format::ini::player_template_ini::PlayerTemplateIni;
use crate::zero_hour::format::ini::terrain_ini::TerrainIni;
use crate::zero_hour::format::ini::weapon_ini::WeaponIni;

/// The install's game data as the game parses it, read once before any stage converts: each kind
/// of its INI files and its string table, which the import's stages read and none writes.
#[derive(Debug, Default)]
pub(crate) struct ZeroHourRules {
    pub(crate) terrain: TerrainIni,
    pub(crate) game_data: GameDataIni,
    pub(crate) mouse: MouseIni,
    pub(crate) objects: ObjectIni,
    pub(crate) locomotors: LocomotorIni,
    pub(crate) templates: PlayerTemplateIni,
    pub(crate) sets: CommandSetIni,
    pub(crate) buttons: CommandButtonIni,
    pub(crate) multiplayer: MultiplayerIni,
    pub(crate) weapons: WeaponIni,
    pub(crate) armors: ArmorIni,
    pub(crate) strings: StringTable,
}

impl ZeroHourRules {
    /// The rules of `game`'s install; an error for a file that does not read as the game reads
    /// it.
    pub(crate) fn read(game: &ZeroHour) -> Result<Self, ZeroHourError> {
        Ok(Self {
            terrain: game.read_inis("Terrain", TerrainIni::default(), TerrainIni::read)?,
            game_data: game.read_inis("GameData", GameDataIni::default(), GameDataIni::read)?,
            mouse: game.read_inis("Mouse", MouseIni::default(), MouseIni::read)?,
            objects: game.read_inis("Object", ObjectIni::default(), ObjectIni::read)?,
            locomotors: game.read_inis("Locomotor", LocomotorIni::default(), LocomotorIni::read)?,
            templates: game.read_inis(
                "PlayerTemplate",
                PlayerTemplateIni::default(),
                PlayerTemplateIni::read,
            )?,
            sets: game.read_inis("CommandSet", CommandSetIni::default(), CommandSetIni::read)?,
            buttons: game.read_inis(
                "CommandButton",
                CommandButtonIni::default(),
                CommandButtonIni::read,
            )?,
            multiplayer: game.read_inis(
                "Multiplayer",
                MultiplayerIni::default(),
                MultiplayerIni::read,
            )?,
            weapons: game.read_inis("Weapon", WeaponIni::default(), WeaponIni::read)?,
            armors: game.read_inis("Armor", ArmorIni::default(), ArmorIni::read)?,
            strings: game.string_table()?,
        })
    }
}
