use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::status::Status;

/// A field of a data file's table, by its name in the file: whether the release reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataField {
    pub table: DataTable,
    pub name: &'static str,
    pub status: Status,
}
