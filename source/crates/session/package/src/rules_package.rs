use campfire_codec::TomlTable;

use crate::files::rules_manifest::RulesNeeds;

/// What a rules package gives a mode that depends on it beside its scripts: what it needs of the
/// mode, and its `data/rules.toml`, mode data that the mode's own lies over, empty without one
/// (03-game-scripting.md, Overrides by load order).
#[derive(Debug)]
pub struct RulesPackage {
    pub needs: RulesNeeds,
    pub data: TomlTable,
}
