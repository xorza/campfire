use serde::de::DeserializeOwned;

use crate::scripts::script_api::data_table::DataTable;

/// A table of package data, by the `DataTable` the script API records it under: what the
/// reference lists and the release tests hold to the schema.
pub(crate) trait DataSection: DeserializeOwned {
    const TABLE: DataTable;
}
