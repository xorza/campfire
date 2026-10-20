use std::fmt;

use campfire_common::NamedEnum;
use serde::de::{Error, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::recipients::Recipients;
use crate::setting::error::VisibilityError;
use crate::setting::{Setting, SettingValue};
use crate::visibility::Visibility;

/// What a session's host sets of its visibility (10-replication-audiences.md, Three layers;
/// 10-replication-audiences.md, The host admits spectators, and their fog is a setting): the
/// settings it gives in place of the mode's, and the most spectators it admits, none unless it
/// says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostVisibility {
    pub settings: Vec<SettingValue>,
    pub max_spectators: u32,
}

/// A mode's `[visibility]` (10-replication-audiences.md, Each kind has a default audience, and a
/// mode may set it; 10-replication-audiences.md, Three layers): its own settings, from the
/// defaults, and those it leaves to the host, `spectators` and `out` unless its `host` list names
/// others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeVisibility {
    settings: Visibility,
    host: Vec<Setting>,
}

impl Default for ModeVisibility {
    fn default() -> Self {
        Self {
            settings: Visibility::DEFAULT,
            host: vec![Setting::Spectators, Setting::Out],
        }
    }
}

impl ModeVisibility {
    /// The key of the settings it leaves to the host.
    const HOST: &'static str = "host";

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

/// Each setting the mode may set by its name and value, then the settings it leaves to the host,
/// which `Deserialize` reads back to it.
impl Serialize for ModeVisibility {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let kinds = Visibility::settable_kinds();
        let mut map = serializer.serialize_map(Some(kinds.clone().count() + 3))?;
        for kind in kinds {
            // A mode sets the part of the audience its kind lets it set; the rest is fixed.
            let fixed = Recipients::ALL.without(kind.settable());
            let set = self.settings.audience(kind).without(fixed);
            let names: Vec<&str> = set.names().collect();
            map.serialize_entry(kind.name(), &names)?;
        }
        map.serialize_entry(
            Setting::Spectators.name(),
            self.settings.spectators().name(),
        )?;
        map.serialize_entry(Setting::Out.name(), self.settings.out().name())?;
        let host: Vec<&str> = self.host.iter().map(|setting| setting.name()).collect();
        map.serialize_entry(Self::HOST, &host)?;
        map.end()
    }
}

/// Each name a setting's, each setting once, each value one its setting takes, as
/// `VisibilityError` names a flaw; `host` a list of settings a mode may set, each once.
impl<'de> Deserialize<'de> for ModeVisibility {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
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
            if name == ModeVisibility::HOST {
                if host.is_some() {
                    return Err(A::Error::duplicate_field(ModeVisibility::HOST));
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
