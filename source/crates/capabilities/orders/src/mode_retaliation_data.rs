use campfire_declarations::{PatchError, Patched, Section, SectionTable};
use campfire_math::Num;
use campfire_script::{ApiVersion, DataSection, DataTable, Status};
use campfire_values::FilterData;
use serde::{Deserialize, Deserializer, Serialize};

/// The mode's `[retaliation]` (04-capabilities/control.md, Retaliation is the mode's data): the
/// units a damaged unit answers, the most meters it lies from them, edge to edge, the radius round
/// it, past its body, within which its allies answer, and how long, in milliseconds, they chase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModeRetaliationData {
    pub aggressors: FilterData,
    pub distance: Num,
    pub radius: Num,
    pub chase_ms: u64,
}

/// The mode's `[retaliation]` as a tool writes it in part, over the rules package's it lies on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeRetaliationPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggressors: Option<FilterData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance: Option<Num>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius: Option<Num>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chase_ms: Option<u64>,
}

impl Patched for ModeRetaliationData {
    type Patch = ModeRetaliationPatch;

    fn from_patch(patch: ModeRetaliationPatch) -> Result<Self, PatchError> {
        Ok(Self {
            aggressors: patch.aggressors.ok_or(PatchError::Missing("aggressors"))?,
            distance: patch.distance.ok_or(PatchError::Missing("distance"))?,
            radius: patch.radius.ok_or(PatchError::Missing("radius"))?,
            chase_ms: patch.chase_ms.ok_or(PatchError::Missing("chase_ms"))?,
        })
    }
}

impl<'de> Deserialize<'de> for ModeRetaliationData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::read_patch(deserializer)
    }
}

impl DataSection for ModeRetaliationData {
    const TABLE: DataTable = DataTable::new("The mode's `[retaliation]`");
}

impl Section for ModeRetaliationData {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "retaliation";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
