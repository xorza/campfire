use campfire_common::NamedEnum;
use campfire_values::Block;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// What is special about an action, beside the pipeline every action runs. The release runs the
/// kinds whose sections its modules register (04-capabilities/actions.md, Each kind owns its
/// section).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, NamedEnum)]
pub enum ActionKind {
    #[default]
    Cast,
    Attack,
    Use,
    Train,
    Build,
    Gather,
}

impl ActionKind {
    /// The block that keeps a unit from starting an action of the kind; a slot of a unit's use
    /// group starts as a use whatever its kind (`UseGroup::group`).
    pub const fn block(self) -> Block {
        match self {
            Self::Cast => Block::Cast,
            Self::Attack => Block::Attack,
            Self::Use | Self::Build | Self::Gather => Block::Use,
            Self::Train => Block::Produce,
        }
    }
}

/// Written and read as serde's derive does a unit variant, as data names a kind.
impl Serialize for ActionKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.serialize_member(serializer)
    }
}

impl<'de> Deserialize<'de> for ActionKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize_member(deserializer)
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
