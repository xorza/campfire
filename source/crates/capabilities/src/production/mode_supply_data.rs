use crate::scripts::data_section::DataSection;
use crate::scripts::script_api::data_table::DataTable;
use serde::Deserialize;

/// The mode's `[supply]`: the mode counts supply, and no player's cap passes `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeSupplyData {
    pub max: u32,
}

impl DataSection for ModeSupplyData {
    const TABLE: DataTable = DataTable::ModeSupply;
}
