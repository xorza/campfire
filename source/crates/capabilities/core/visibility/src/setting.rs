use std::fmt;
use std::str::FromStr;

use campfire_state_types::{DataKind, Recipients};

use crate::error::VisibilityError;
use crate::{OutView, SpectatorFog};

/// A setting of a session's visibility, which a mode's `[visibility]` sets and its `host` list may
/// leave to the host (design 14, D4, D14): a kind's audience, the spectators' fog, or what a player
/// out of play sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Setting {
    Kind(DataKind),
    Spectators,
    Out,
}

/// A setting with the value it takes: a kind's recipients that the mode may set, the spectators'
/// fog, or what a player out of play sees. A host gives one as `--visibility` writes it:
/// `name=value`, a kind's recipients separated by commas, none for the owner alone;
/// `spectators=all` or `teams`; `out=team` or `spectators`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingValue {
    Audience(DataKind, Recipients),
    Spectators(SpectatorFog),
    Out(OutView),
}

impl Setting {
    /// The setting of the name `name`; an error for no setting's, and for a kind whose audience
    /// no mode sets.
    pub fn named(name: &str) -> Result<Setting, VisibilityError> {
        match name {
            "spectators" => Ok(Setting::Spectators),
            "out" => Ok(Setting::Out),
            name => match DataKind::named(name) {
                Some(kind) if kind.settable() == Recipients::NONE => {
                    Err(VisibilityError::NotSettable(kind))
                }
                Some(kind) => Ok(Setting::Kind(kind)),
                None => Err(VisibilityError::UnknownSetting(name.to_owned())),
            },
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Setting::Kind(kind) => kind.name(),
            Setting::Spectators => "spectators",
            Setting::Out => "out",
        }
    }

    /// The value of the setting that `names` give: for a kind, its recipients' names, each once;
    /// for the others, the one name of a value. An error for a name the setting does not take.
    pub fn value<'n>(
        self,
        names: impl IntoIterator<Item = &'n str>,
    ) -> Result<SettingValue, VisibilityError> {
        let mut names = names.into_iter();
        let unknown = |value: &str| VisibilityError::UnknownValue {
            setting: self,
            value: value.to_owned(),
        };
        let mut one = |named: fn(&str) -> Option<SettingValue>| match (names.next(), names.next()) {
            (Some(name), None) => named(name).ok_or_else(|| unknown(name)),
            (first, second) => {
                let given: Vec<&str> = first.into_iter().chain(second).collect();
                Err(unknown(&given.join(",")))
            }
        };
        match self {
            Setting::Spectators => {
                one(|name| SpectatorFog::named(name).map(SettingValue::Spectators))
            }
            Setting::Out => one(|name| OutView::named(name).map(SettingValue::Out)),
            Setting::Kind(kind) => {
                let mut recipients = Recipients::NONE;
                for name in names {
                    let recipient = Recipients::named(name).ok_or_else(|| unknown(name))?;
                    if recipients.contains(recipient) {
                        let recipient = recipient.names().next().expect("one recipient");
                        return Err(VisibilityError::RepeatedRecipient { kind, recipient });
                    }
                    recipients = recipients | recipient;
                }
                Ok(SettingValue::Audience(kind, recipients))
            }
        }
    }
}

impl fmt::Display for Setting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl SettingValue {
    pub const fn setting(self) -> Setting {
        match self {
            SettingValue::Audience(kind, _) => Setting::Kind(kind),
            SettingValue::Spectators(_) => Setting::Spectators,
            SettingValue::Out(_) => Setting::Out,
        }
    }
}

impl FromStr for SettingValue {
    type Err = VisibilityError;

    fn from_str(text: &str) -> Result<SettingValue, VisibilityError> {
        let (name, value) = text
            .split_once('=')
            .ok_or_else(|| VisibilityError::NotHostSetting(text.to_owned()))?;
        let setting = Setting::named(name)?;
        // An empty value names no recipient; an empty name among others is no recipient's.
        let names = (!value.is_empty())
            .then(|| value.split(','))
            .into_iter()
            .flatten();
        setting.value(names)
    }
}
