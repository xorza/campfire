use std::collections::BTreeMap;
use std::ops::Index;

use campfire_client_files::ModelScale;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{IniLine, IniText};
use crate::zero_hour::format::ini::object_ini::fight_data::FightData;
use crate::zero_hour::format::ini::object_ini::geometry::Geometry;
use crate::zero_hour::format::ini::object_ini::ini_build::IniBuild;
use crate::zero_hour::format::ini::object_ini::ini_experience::IniExperience;
use crate::zero_hour::format::ini::object_ini::ini_object::{IniObject, RadarPriority};
use crate::zero_hour::format::ini::object_ini::ini_power::IniPowers;
use crate::zero_hour::format::ini::object_ini::ini_supply::IniSupply;
use crate::zero_hour::format::ini::object_ini::ini_voices::IniVoices;
use crate::zero_hour::format::ini::object_ini::object_kinds::ObjectKinds;
use crate::zero_hour::format::ini::object_ini::object_reading::{
    OPEN_ALONE, OPEN_WITH_VALUE, OpenObject, Reading,
};

pub(crate) mod draw_module;
pub(crate) mod fight_data;
pub(crate) mod geometry;
pub(crate) mod ini_animation;
pub(crate) mod ini_build;
pub(crate) mod ini_contain;
pub(crate) mod ini_death;
pub(crate) mod ini_experience;
pub(crate) mod ini_fall;
pub(crate) mod ini_object;
pub(crate) mod ini_power;
pub(crate) mod ini_propaganda;
pub(crate) mod ini_recoil;
pub(crate) mod ini_supply;
pub(crate) mod ini_upgrade_module;
pub(crate) mod ini_voices;
pub(crate) mod object_kinds;
pub(crate) mod object_reading;

/// The objects of Zero Hour's object INI files, as `ThingFactory` reads them, each with what its
/// draw modules show by default, every file read.
#[derive(Debug, Default)]
pub(crate) struct ObjectIni {
    files: ObjectFiles,
}

/// The object INI files read so far, in their order: what `finish` makes the `ObjectIni` of.
#[derive(Debug, Default)]
pub(crate) struct ObjectFiles {
    /// Each object, in the order the files define them, at its id.
    objects: Vec<IniObject>,
    /// Each object's id by its name, of its case, as `ThingFactory::findTemplate` finds it.
    by_name: BTreeMap<String, ObjectId>,
    /// The files read so far.
    files: u32,
}

/// An object INI file, by its place in the order the files are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IniFile(u32);

/// An object of the INI files, by its place in their order: what every stage names an object by,
/// as the files hold one object of a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ObjectId(u32);

/// The weapon slots, as a field names them.
pub(crate) const SLOTS: [&str; 3] = ["primary", "secondary", "tertiary"];

impl ObjectFiles {
    /// Reads one INI file, adding its objects to those read before. An object of a name read
    /// before, and a reskin of a base not yet read, are refused: no known install holds either,
    /// where the game's release build parses the second definition into the first, and refuses
    /// a reskin of no base.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        let text = IniText::of(bytes);
        let mut reading = Reading::default();
        for line in text.lines().iter() {
            let word = line.word();
            if word == "end" {
                if let Some(done) = reading.end(line.number)? {
                    debug_assert_eq!(done.id, self.next_id(), "an object opens with the next id");
                    self.by_name.insert(done.name.clone(), done.id);
                    self.objects.push(done);
                }
            } else if reading.stack.is_empty() {
                reading.open(self.object_of(word, &line)?);
            } else {
                reading.line(word, &line)?;
            }
        }
        self.files += 1;
        if reading.stack.is_empty() {
            Ok(())
        } else {
            Err(IniError::Unclosed)
        }
    }

    /// The objects read, every file read.
    pub(crate) const fn finish(self) -> ObjectIni {
        ObjectIni { files: self }
    }

    /// Every object, in the order of their names.
    pub(crate) fn objects(&self) -> impl Iterator<Item = &IniObject> {
        self.by_name.values().map(|&id| &self[id])
    }

    /// The object of the name `name`, of its case; none for a name no object has.
    pub(crate) fn named(&self, name: &str) -> Option<&IniObject> {
        self.by_name.get(name).map(|&id| &self[id])
    }

    /// The id the next object read takes.
    fn next_id(&self) -> ObjectId {
        ObjectId(
            u32::try_from(self.objects.len()).expect("an install holds fewer than 2³² objects"),
        )
    }

    /// The object a top-level line of `word` opens: a new one, or a copy of the reskin's base.
    fn object_of(&self, word: &str, at: &IniLine<'_>) -> Result<OpenObject, IniError> {
        let line = at.number;
        let base = match word {
            "object" => None,
            "objectreskin" => Some(*at.tokens.get(2).ok_or(IniError::NoName { line })?),
            _ => return Err(IniError::NotBlock { line }),
        };
        let name = *at.tokens.get(1).ok_or(IniError::NoName { line })?;
        if self.by_name.contains_key(name) {
            return Err(IniError::Redefined { line });
        }
        let id = self.next_id();
        let object = match base {
            None => IniObject {
                id,
                name: name.to_owned(),
                file: IniFile(self.files),
                draws: Vec::new(),
                kinds: ObjectKinds::default(),
                fence_width: 0.0,
                scale: ModelScale::ONE,
                is_bridge: false,
                geometry: Geometry::DEFAULT,
                locomotors: Vec::new(),
                taxi_locomotors: Vec::new(),
                build: IniBuild::default(),
                supply: IniSupply::default(),
                fight: FightData::default(),
                upgrades: Vec::new(),
                deaths: Vec::new(),
                mass: 1.0,
                lateral_friction: None,
                lifetime_ms: None,
                propaganda: None,
                voices: IniVoices::default(),
                vision_range: 0.0,
                shroud_clearing_range: -1.0,
                granted: Vec::new(),
                radar_priority: RadarPriority::Unset,
                contain: None,
                transport_slots: 0,
                button_image: None,
                experience: IniExperience::default(),
                powers: IniPowers::default(),
            },
            Some(base) => IniObject {
                id,
                name: name.to_owned(),
                file: IniFile(self.files),
                ..self.named(base).ok_or(IniError::NoBase { line })?.clone()
            },
        };
        Ok(OpenObject {
            object,
            reskin: base.is_some(),
            copied_draws: base.is_some(),
        })
    }

    /// Whether the line of `word` and `tokens` opens a block.
    fn opens(word: &str, tokens: &[&str]) -> bool {
        OPEN_WITH_VALUE.contains(&word) || (tokens.len() == 1 && OPEN_ALONE.contains(&word))
    }

    /// A model field's value, in lowercase, as the game finds a model's file whatever its case:
    /// none for `NONE`.
    fn model(tokens: &[&str]) -> Option<String> {
        tokens
            .get(1)
            .filter(|model| !model.eq_ignore_ascii_case("none"))
            .map(|model| model.to_ascii_lowercase())
    }
}

impl Index<ObjectId> for ObjectFiles {
    type Output = IniObject;

    fn index(&self, id: ObjectId) -> &IniObject {
        &self.objects[usize::try_from(id.0).expect("a u32 fits usize")]
    }
}

impl ObjectIni {
    /// Every object, in the order of their names.
    pub(crate) fn objects(&self) -> impl Iterator<Item = &IniObject> {
        self.files.objects()
    }

    /// The object a map's template names, of its case, as `findTemplate` finds it; none for a
    /// template no INI object names, which the game spawns nothing of.
    pub(crate) fn object(&self, template: &[u8]) -> Option<&IniObject> {
        let name: String = template.iter().map(|&byte| char::from(byte)).collect();
        self.named(&name)
    }

    /// The object of the name `name`, of its case; none for a name no object has.
    pub(crate) fn named(&self, name: &str) -> Option<&IniObject> {
        self.files.named(name)
    }
}

impl Index<ObjectId> for ObjectIni {
    type Output = IniObject;

    fn index(&self, id: ObjectId) -> &IniObject {
        &self.files[id]
    }
}

#[cfg(test)]
pub(crate) mod samples;

#[cfg(test)]
mod tests;
