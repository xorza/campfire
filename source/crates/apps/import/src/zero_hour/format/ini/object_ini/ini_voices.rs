use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;

/// The audio events an object answers with, each by its name, none where it names none or `NONE`:
/// its `VoiceSelect`, `VoiceMove` and `VoiceAttack`, which a selection and an order play
/// (`pickAndPlayUnitVoiceResponse`); its `VoiceCreated` and its `UnitSpecificSounds`'
/// `VoiceCreate`, both of which a unit it is plays as its producer makes it (`ProductionUpdate`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct IniVoices {
    pub(crate) select: Option<String>,
    pub(crate) walk: Option<String>,
    pub(crate) attack: Option<String>,
    pub(crate) created: Option<String>,
    pub(crate) create: Option<String>,
}

impl IniVoices {
    /// Takes an object's field `word`; whether it was one.
    pub(crate) fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<bool, IniError> {
        let field = match word {
            "voiceselect" => &mut self.select,
            "voicemove" => &mut self.walk,
            "voiceattack" => &mut self.attack,
            "voicecreated" => &mut self.created,
            _ => return Ok(false),
        };
        *field = Self::event(at)?;
        Ok(true)
    }

    /// Takes a line of its `UnitSpecificSounds` block.
    pub(crate) fn unit_specific(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        if word == "voicecreate" {
            self.create = Self::event(at)?;
        }
        Ok(())
    }

    /// The event a line names, none for `NONE`.
    fn event(at: &IniLine<'_>) -> Result<Option<String>, IniError> {
        let name = at.value()?;
        Ok((!name.eq_ignore_ascii_case("none")).then(|| name.to_owned()))
    }
}
