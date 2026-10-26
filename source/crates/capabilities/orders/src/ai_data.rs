use campfire_declarations::{Section, SectionTable};
use campfire_script::{ApiVersion, Status};
use campfire_script::{DataSection, DataTable};
use campfire_values::ScriptSource;
use serde::Deserialize;

/// A unit type's AI as its `orders` section declares it: the script whose `on_think` runs, its
/// package's own or a rules package's, and how often, in milliseconds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiData {
    pub ai: ScriptSource,
    pub think_ms: u64,
}

impl DataSection for AiData {
    const TABLE: DataTable = DataTable::new("A unit type's `orders`");
}

impl Section for AiData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "orders";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
