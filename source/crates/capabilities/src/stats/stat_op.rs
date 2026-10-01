use serde::{Deserialize, Serialize};

/// How a modifier changes a stat a stack: it adds to the value, adds a percent that multiplies
/// it, or cuts it, only the largest cut counting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatOp {
    Add,
    Pct,
    Cut,
}
