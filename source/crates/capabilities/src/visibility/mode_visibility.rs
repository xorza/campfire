use std::fmt;

use serde::de::{Error, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::visibility::Visibility;
use crate::visibility::error::VisibilityError;
use crate::visibility::setting::{Setting, SettingValue};

/// What a session's host sets of its visibility (design 14, D14, D15): the settings it gives in
/// place of the mode's, and the most spectators it admits, none unless it says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostVisibility {
    pub settings: Vec<SettingValue>,
    pub max_spectators: u32,
}

/// A mode's `[visibility]` (design 14, D4, D14): its own settings, from the defaults, and those
/// it leaves to the host, `spectators` and `out` unless its `host` list names others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeVisibility {
    settings: Visibility,
    host: Vec<Setting>,
}

impl Default for ModeVisibility {
    fn default() -> ModeVisibility {
        ModeVisibility {
            settings: Visibility::DEFAULT,
            host: vec![Setting::Spectators, Setting::Out],
        }
    }
}

impl ModeVisibility {
    /// The mode's own settings, which admit no spectator.
    pub const fn settings(&self) -> &Visibility {
        &self.settings
    }

    /// Whether the host may set `setting` again.
    pub fn leaves(&self, setting: Setting) -> bool {
        self.host.contains(&setting)
    }

    /// Whether a session of the mode may hold `visibility`: each setting the mode does not leave
    /// to the host is the mode's own; any spectator count.
    pub fn admits(&self, visibility: &Visibility) -> bool {
        let own = &self.settings;
        let mut kinds =
            Visibility::settable_kinds().filter(|&kind| !self.leaves(Setting::Kind(kind)));
        kinds.all(|kind| visibility.audience(kind) == own.audience(kind))
            && (self.leaves(Setting::Spectators) || visibility.spectators() == own.spectators())
            && (self.leaves(Setting::Out) || visibility.out() == own.out())
    }

    /// The session's visibility: the mode's settings, each `host` gives in place of the mode's,
    /// and the spectators it admits. An error for a setting the mode does not leave to the host,
    /// for one the host gives twice, and for a recipient no mode sets for its kind.
    pub fn session(&self, host: &HostVisibility) -> Result<Visibility, VisibilityError> {
        let mut visibility = self.settings;
        for (at, &given) in host.settings.iter().enumerate() {
            let setting = given.setting();
            if !self.leaves(setting) {
                return Err(VisibilityError::NotLeft(setting));
            }
            let earlier = &host.settings[..at];
            if earlier.iter().any(|earlier| earlier.setting() == setting) {
                return Err(VisibilityError::Repeated(setting));
            }
            visibility.set(given)?;
        }
        Ok(visibility.with_max_spectators(host.max_spectators))
    }
}

/// Each name a setting's, each setting once, each value one its setting takes, as
/// `VisibilityError` names a flaw; `host` a list of settings a mode may set, each once.
impl<'de> Deserialize<'de> for ModeVisibility {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ModeVisibility, D::Error> {
        deserializer.deserialize_map(ModeVisibilityVisitor)
    }
}

/// Reads a `[visibility]` table, `ModeVisibility`'s `Deserialize`.
#[derive(Debug)]
struct ModeVisibilityVisitor;

impl<'de> Visitor<'de> for ModeVisibilityVisitor {
    type Value = ModeVisibility;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a table of visibility settings")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<ModeVisibility, A::Error> {
        let mut visibility = ModeVisibility::default();
        let mut host = None;
        let mut set = Vec::new();
        while let Some(name) = map.next_key::<String>()? {
            if name == "host" {
                if host.is_some() {
                    return Err(A::Error::duplicate_field("host"));
                }
                let mut settings = Vec::new();
                for name in map.next_value::<Vec<String>>()? {
                    let setting = Setting::named(&name).map_err(A::Error::custom)?;
                    if settings.contains(&setting) {
                        return Err(A::Error::custom(VisibilityError::Repeated(setting)));
                    }
                    settings.push(setting);
                }
                host = Some(settings);
                continue;
            }
            let setting = Setting::named(&name).map_err(A::Error::custom)?;
            if set.contains(&setting) {
                return Err(A::Error::custom(VisibilityError::Repeated(setting)));
            }
            set.push(setting);
            let value = match setting {
                Setting::Kind(_) => {
                    let names = map.next_value::<Vec<String>>()?;
                    setting.value(names.iter().map(String::as_str))
                }
                Setting::Spectators | Setting::Out => {
                    let name = map.next_value::<String>()?;
                    setting.value([name.as_str()])
                }
            };
            let value = value.map_err(A::Error::custom)?;
            visibility.settings.set(value).map_err(A::Error::custom)?;
        }
        if let Some(host) = host {
            visibility.host = host;
        }
        Ok(visibility)
    }
}
