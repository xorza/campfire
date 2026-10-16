use campfire_declare::PerType;
use campfire_script::Filter;
use campfire_sim::Capability;
use campfire_values::{ActionId, Relation, TagSet};

/// An object type's use as a match reads it, package data and not state: the action of the `use`
/// kind a user's use slot runs at it, and the units that may use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UseSpec {
    action: ActionId,
    users: Filter,
}

impl UseSpec {
    pub const fn new(action: ActionId, users: Filter) -> Self {
        Self { action, users }
    }

    pub const fn action(self) -> ActionId {
        self.action
    }

    /// Whether its users select a unit of `tags` whose team the object's team regards with
    /// `relation`.
    pub const fn admits(self, relation: Relation, tags: TagSet) -> bool {
        self.users.selects(relation, tags)
    }
}

impl PerType for UseSpec {
    const OWNER: Option<Capability> = Some(Capability::Interaction);
    const SCRIPTS_ONLY: bool = false;
}
