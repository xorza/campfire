use campfire_declarations::{PatchError, Patched, Section, SectionTable};
use campfire_math::Num;
use campfire_script::{ApiVersion, DataSection, DataTable, Status};
use serde::{Deserialize, Deserializer, Serialize};

/// A unit type's `retaliate`, which makes its units answer an ally's call
/// (04-capabilities/control.md, Retaliation is the mode's data): the meters from where it stood
/// past which it gives up, its leash, and those past which its target lies when it gives up, its
/// reach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RetaliateData {
    pub leash: Num,
    pub reach: Num,
}

/// A unit type's `retaliate` as a tool writes it in part, over the defaults it lies on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetaliatePatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leash: Option<Num>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reach: Option<Num>,
}

impl Patched for RetaliateData {
    type Patch = RetaliatePatch;

    fn from_patch(patch: RetaliatePatch) -> Result<Self, PatchError> {
        Ok(Self {
            leash: patch.leash.ok_or(PatchError::Missing("leash"))?,
            reach: patch.reach.ok_or(PatchError::Missing("reach"))?,
        })
    }
}

impl<'de> Deserialize<'de> for RetaliateData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::read_patch(deserializer)
    }
}

impl DataSection for RetaliateData {
    const TABLE: DataTable = DataTable::new("A unit type's `retaliate`");
}

impl Section for RetaliateData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "retaliate";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
