use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::csf::string_table::StringTable;
use crate::zero_hour::format::ini::armor_ini::ArmorIni;
use crate::zero_hour::format::ini::audio_ini::AudioIni;
use crate::zero_hour::format::ini::command_button_ini::CommandButtonIni;
use crate::zero_hour::format::ini::command_set_ini::CommandSetIni;
use crate::zero_hour::format::ini::creation_list_ini::CreationListIni;
use crate::zero_hour::format::ini::fx_list_ini::FxListIni;
use crate::zero_hour::format::ini::game_data_ini::GameDataIni;
use crate::zero_hour::format::ini::locomotor_ini::LocomotorIni;
use crate::zero_hour::format::ini::mapped_image_ini::MappedImageIni;
use crate::zero_hour::format::ini::mouse_ini::MouseIni;
use crate::zero_hour::format::ini::multiplayer_ini::MultiplayerIni;
use crate::zero_hour::format::ini::object_ini::{ObjectFiles, ObjectIni};
use crate::zero_hour::format::ini::particle_system_ini::ParticleSystemIni;
use crate::zero_hour::format::ini::player_template_ini::PlayerTemplateIni;
use crate::zero_hour::format::ini::roads_ini::RoadsIni;
use crate::zero_hour::format::ini::terrain_ini::TerrainIni;
use crate::zero_hour::format::ini::upgrade_ini::UpgradeIni;
use crate::zero_hour::format::ini::weapon_ini::WeaponIni;
use crate::zero_hour::install::Install;
use crate::zero_hour::rules_names::RulesNames;

/// The install's game data as the game parses it, read once before any stage converts: each kind
/// of its INI files and its string table, which the import's stages read and none writes.
#[derive(Debug, Default)]
pub(crate) struct ZeroHourRules {
    pub(crate) terrain: TerrainIni,
    pub(crate) roads: RoadsIni,
    pub(crate) game_data: GameDataIni,
    pub(crate) mouse: MouseIni,
    pub(crate) objects: ObjectIni,
    pub(crate) locomotors: LocomotorIni,
    pub(crate) templates: PlayerTemplateIni,
    pub(crate) sets: CommandSetIni,
    pub(crate) buttons: CommandButtonIni,
    pub(crate) images: MappedImageIni,
    pub(crate) upgrades: UpgradeIni,
    pub(crate) multiplayer: MultiplayerIni,
    pub(crate) weapons: WeaponIni,
    pub(crate) armors: ArmorIni,
    pub(crate) particles: ParticleSystemIni,
    pub(crate) effects: FxListIni,
    pub(crate) creation_lists: CreationListIni,
    pub(crate) audio: AudioIni,
    pub(crate) strings: StringTable,
    pub(crate) names: RulesNames,
}

impl ZeroHourRules {
    /// The rules of `install`; an error for a file that does not read as the game reads it.
    pub(crate) fn read(install: &Install) -> Result<Self, ZeroHourError> {
        Ok(Self {
            objects: install
                .read_inis("Object", ObjectFiles::default(), ObjectFiles::read)?
                .finish(),
            terrain: install.read_inis("Terrain", TerrainIni::default(), TerrainIni::read)?,
            roads: install.read_inis("Roads", RoadsIni::default(), RoadsIni::read)?,
            game_data: install.read_inis("GameData", GameDataIni::default(), GameDataIni::read)?,
            mouse: install.read_inis("Mouse", MouseIni::default(), MouseIni::read)?,
            locomotors: install.read_inis(
                "Locomotor",
                LocomotorIni::default(),
                LocomotorIni::read,
            )?,
            templates: install.read_inis(
                "PlayerTemplate",
                PlayerTemplateIni::default(),
                PlayerTemplateIni::read,
            )?,
            sets: install.read_inis("CommandSet", CommandSetIni::default(), CommandSetIni::read)?,
            buttons: install.read_inis(
                "CommandButton",
                CommandButtonIni::default(),
                CommandButtonIni::read,
            )?,
            images: Self::images(install)?,
            upgrades: install.read_inis("Upgrade", UpgradeIni::default(), UpgradeIni::read)?,
            multiplayer: install.read_inis(
                "Multiplayer",
                MultiplayerIni::default(),
                MultiplayerIni::read,
            )?,
            weapons: install.read_inis("Weapon", WeaponIni::default(), WeaponIni::read)?,
            armors: install.read_inis("Armor", ArmorIni::default(), ArmorIni::read)?,
            particles: install.read_inis(
                "ParticleSystem",
                ParticleSystemIni::default(),
                ParticleSystemIni::read,
            )?,
            effects: install.read_inis("FXList", FxListIni::default(), FxListIni::read)?,
            creation_lists: install.read_inis(
                "ObjectCreationList",
                CreationListIni::default(),
                CreationListIni::read,
            )?,
            audio: Self::audio(install)?,
            strings: Self::string_table(install)?,
            names: RulesNames::default(),
        })
    }

    /// The install's mapped images, those of its textures of 512 pixels, then those made by hand,
    /// as `ImageCollection::load` reads them at the game's full detail.
    fn images(install: &Install) -> Result<MappedImageIni, ZeroHourError> {
        let sized = install.read_inis(
            "MappedImages\\TextureSize_512",
            MappedImageIni::default(),
            MappedImageIni::read,
        )?;
        install.read_inis("MappedImages\\HandCreated", sized, MappedImageIni::read)
    }

    /// The install's audio events, of its sound effects and its voices, its music, and its
    /// settings, as `AudioManager::init` reads them.
    fn audio(install: &Install) -> Result<AudioIni, ZeroHourError> {
        let audio = install.read_inis("SoundEffects", AudioIni::default(), AudioIni::read)?;
        let audio = install.read_inis("Voice", audio, AudioIni::read)?;
        let audio = install.read_inis("Music", audio, AudioIni::read)?;
        install.read_inis("AudioSettings", audio, AudioIni::read_settings)
    }

    /// The install's string table, `Data\<language>\generals.csf`, as the game reads its
    /// language's.
    fn string_table(install: &Install) -> Result<StringTable, ZeroHourError> {
        let language = install.language()?.ok_or(ZeroHourError::NoLanguage)?;
        let path = ArchivePath::of(&format!("data\\{language}\\generals.csf"));
        let bytes = install.read(&path)?;
        StringTable::read(&bytes).map_err(|error| ZeroHourError::StringTable { path, error })
    }
}
