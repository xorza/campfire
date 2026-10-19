use campfire_common::EnumAll;
use campfire_values::Block;
use serde::{Deserialize, Serialize};

/// What is special about an action, beside the pipeline every action runs. The release runs the
/// kinds whose sections its modules register (04-capabilities/actions.md, Each kind owns its
/// section).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, EnumAll)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    #[default]
    Cast,
    Attack,
    Use,
    Enter,
    Train,
    Build,
    Gather,
    Craft,
}

impl ActionKind {
    /// The kind's name, as data writes it and as its section is registered by.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cast => "cast",
            Self::Attack => "attack",
            Self::Use => "use",
            Self::Enter => "enter",
            Self::Train => "train",
            Self::Build => "build",
            Self::Gather => "gather",
            Self::Craft => "craft",
        }
    }

    /// The block that keeps a unit from starting an action of the kind; a slot of a unit's use
    /// group starts as a use whatever its kind (`UseGroup::group`).
    pub const fn block(self) -> Block {
        match self {
            Self::Cast | Self::Craft => Block::Cast,
            Self::Attack => Block::Attack,
            Self::Use | Self::Enter | Self::Build | Self::Gather => Block::Use,
            Self::Train => Block::Produce,
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;
    use serde::Deserialize;

    use super::*;

    #[test]
    fn each_kind_is_named_as_data_writes_it() {
        #[derive(Debug, Deserialize)]
        struct Written {
            kind: ActionKind,
        }
        for kind in ActionKind::ALL {
            let text = format!("kind = \"{}\"", kind.name());
            assert_eq!(Toml::parse::<Written>(&text).unwrap().kind, kind);
        }
    }
}
