use campfire_declarations::{PatchError, Patched, Section, SectionTable};
use campfire_script::{DataSection, DataTable};
use campfire_values::FilterData;
use serde::{Deserialize, Deserializer, Serialize};

/// A unit type's idle scan as its `acquire` section declares it: how often, in milliseconds, a
/// unit of it that stands idle looks for a target, and the units it may take, which any one of
/// the filters selects (04-capabilities/control.md, An idle unit scans for a target, as its type
/// declares).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcquireData {
    pub scan_ms: u64,
    pub targets: Vec<FilterData>,
}

/// A unit type's `acquire` as a tool writes it in part, over the defaults it lies on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquirePatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<FilterData>>,
}

impl Patched for AcquireData {
    type Patch = AcquirePatch;

    fn from_patch(patch: AcquirePatch) -> Result<Self, PatchError> {
        Ok(Self {
            scan_ms: patch.scan_ms.ok_or(PatchError::Missing("scan_ms"))?,
            targets: patch.targets.ok_or(PatchError::Missing("targets"))?,
        })
    }
}

impl<'de> Deserialize<'de> for AcquireData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::read_patch(deserializer)
    }
}

impl DataSection for AcquireData {
    const TABLE: DataTable = DataTable::new("A unit type's `acquire`");
}

impl Section for AcquireData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "acquire";
}
