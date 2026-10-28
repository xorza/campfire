use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;

/// An object's veterancy, as `ThingTemplate` reads it: the experience its death gives at each
/// level from regular, `ExperienceValue`, the experience each level needs, `ExperienceRequired`,
/// each list of exactly four as `parseIntList` reads them, 0 by default; whether it gains
/// experience, `IsTrainable`, no by default; and its `VeterancyGainCreate` modules' starts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct IniExperience {
    pub(crate) values: [i32; 4],
    pub(crate) required: [i32; 4],
    pub(crate) trainable: bool,
    pub(crate) starts: Vec<IniStart>,
}

/// A `VeterancyGainCreate` module: the level its object is made at, `StartingLevel`, of
/// `TheVeterancyNames` from `REGULAR`, 0, and the science its player needs for it,
/// `ScienceRequired`, none by default or of `NONE`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct IniStart {
    pub(crate) level: u8,
    pub(crate) science: Option<String>,
}

impl IniExperience {
    /// Takes the line of `word`, one of its three fields.
    pub(crate) fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        let list = |at: &IniLine<'_>| -> Result<[i32; 4], IniError> {
            Ok([at.int_at(1)?, at.int_at(2)?, at.int_at(3)?, at.int_at(4)?])
        };
        match word {
            "experiencevalue" => self.values = list(at)?,
            "experiencerequired" => self.required = list(at)?,
            "istrainable" => self.trainable = at.yes()?,
            _ => unreachable!("an experience field"),
        }
        Ok(())
    }

    /// Takes the line of `word` of its last `VeterancyGainCreate` module, a field it reads or
    /// another; a level of no veterancy's name is refused, as `INI::parseIndexList` refuses one.
    pub(crate) fn start_field(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        let start = self
            .starts
            .last_mut()
            .expect("a module's field follows its module");
        match word {
            "startinglevel" => {
                let names = ["REGULAR", "VETERAN", "ELITE", "HEROIC"];
                let value = at.value()?;
                let level = names
                    .iter()
                    .position(|name| name.eq_ignore_ascii_case(value))
                    .ok_or(IniError::NotListed { line: at.number })?;
                start.level = u8::try_from(level).expect("four levels fit u8");
            }
            "sciencerequired" => {
                let value = at.value()?;
                start.science = (!value.eq_ignore_ascii_case("none")).then(|| value.to_owned());
            }
            _ => {}
        }
        Ok(())
    }
}
