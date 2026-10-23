use crate::package_name::PackageName;

/// A record of mode data that more than one package gives, and the package whose value the mode
/// takes, the last loaded (03-game-scripting.md, Overrides by load order): a key of the data, or a
/// named entry of one, as `state.phase`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataOverride {
    pub record: String,
    pub by: PackageName,
}
