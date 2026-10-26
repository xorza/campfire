use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_values::PackagePath;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::action_look::ActionLook;
use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;
use crate::client_unit::{ClientUnit, LookDraw};
use crate::condition_test::ConditionTest;
use crate::error::UntestedCondition;
use crate::object_look::ObjectLook;

/// A package's `client/units.toml`: how each unit type looks, `[units.<type>]`; what each
/// condition its looks' states name tests of a unit, `[conditions.<name>]`; each look its
/// maps' client objects name, `[objects.<look>]`, which a game may draw otherwise than its units
/// (09-zero-hour.md, The multiplayer rule for client objects); and what each action plays,
/// `[actions.<action>]`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct ClientUnits {
    units: BTreeMap<DeclaredName, ClientUnit>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    conditions: BTreeMap<DeclaredName, ConditionTest>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    objects: BTreeMap<DeclaredName, ObjectLook>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    actions: BTreeMap<DeclaredName, ActionLook>,
}

impl ClientUnits {
    /// The looks of `units`, the tests of `conditions` and the looks of `objects`; refused when a
    /// draw of a unit's look names a condition that `conditions` gives no test, as a read refuses
    /// it.
    pub fn new(
        units: BTreeMap<DeclaredName, ClientUnit>,
        conditions: BTreeMap<DeclaredName, ConditionTest>,
        objects: BTreeMap<DeclaredName, ObjectLook>,
    ) -> Result<Self, UntestedCondition> {
        let looks = Self {
            units,
            conditions,
            objects,
            actions: BTreeMap::new(),
        };
        match looks.untested() {
            Some(name) => Err(UntestedCondition(name.clone())),
            None => Ok(looks),
        }
    }

    pub const fn units(&self) -> &BTreeMap<DeclaredName, ClientUnit> {
        &self.units
    }

    pub const fn conditions(&self) -> &BTreeMap<DeclaredName, ConditionTest> {
        &self.conditions
    }

    pub const fn objects(&self) -> &BTreeMap<DeclaredName, ObjectLook> {
        &self.objects
    }

    pub const fn actions(&self) -> &BTreeMap<DeclaredName, ActionLook> {
        &self.actions
    }

    /// The looks, each action playing as `actions` says.
    #[must_use]
    pub fn with_actions(self, actions: BTreeMap<DeclaredName, ActionLook>) -> Self {
        Self { actions, ..self }
    }

    /// Settles each object look on the map's `conditions`, which hold for the whole match.
    pub fn settle_objects(&mut self, conditions: &[DeclaredName]) {
        for look in self.objects.values_mut() {
            look.settle(conditions);
        }
    }

    /// Where a package holds it.
    pub fn path() -> PackagePath {
        client_path::parse("client/units.toml")
    }

    /// The first condition a draw of its looks names, of a state or of its ignore, that its
    /// conditions give no test, so no client could tell whether a unit holds it.
    fn untested(&self) -> Option<&DeclaredName> {
        let mut names = self
            .units
            .values()
            .flat_map(ClientUnit::every_draw)
            .flat_map(LookDraw::conditions);
        names.find(|name| !self.conditions.contains_key(*name))
    }
}

impl ClientFile for ClientUnits {
    const FORMAT: ClientFormat = ClientFormat::Toml;
}

/// A package's file is untrusted, so a state that names a condition of no test fails to read.
impl<'de> Deserialize<'de> for ClientUnits {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            units: BTreeMap<DeclaredName, ClientUnit>,
            #[serde(default)]
            conditions: BTreeMap<DeclaredName, ConditionTest>,
            #[serde(default)]
            objects: BTreeMap<DeclaredName, ObjectLook>,
            #[serde(default)]
            actions: BTreeMap<DeclaredName, ActionLook>,
        }
        let Fields {
            units,
            conditions,
            objects,
            actions,
        } = Fields::deserialize(deserializer)?;
        let looks = Self::new(units, conditions, objects).map_err(D::Error::custom)?;
        Ok(looks.with_actions(actions))
    }
}

#[cfg(test)]
mod tests;
