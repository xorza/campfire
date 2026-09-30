use bevy_ecs::resource::Resource;
use campfire_sim::{PlayerSlot, SimResource};
use serde::{Deserialize, Serialize};

/// The players' named resources, such as gold: one run sorted by slot, then by name.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerResources(Vec<PlayerResource>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerResource {
    pub slot: PlayerSlot,
    pub name: String,
    pub amount: i64,
}

impl PlayerResources {
    /// The amount of `name` player `slot` holds; 0 before any is added.
    pub fn amount(&self, slot: PlayerSlot, name: &str) -> i64 {
        self.find(slot, name).map_or(0, |at| self.0[at].amount)
    }

    /// Adds `amount`; `None` when the sum overflows, which leaves it unchanged.
    pub(crate) fn add(&mut self, slot: PlayerSlot, name: &str, amount: i64) -> Option<()> {
        match self.find(slot, name) {
            Ok(at) => {
                self.0[at].amount = self.0[at].amount.checked_add(amount)?;
            }
            Err(at) => self.0.insert(
                at,
                PlayerResource {
                    slot,
                    name: name.to_owned(),
                    amount,
                },
            ),
        }
        Some(())
    }

    fn find(&self, slot: PlayerSlot, name: &str) -> Result<usize, usize> {
        self.0
            .binary_search_by(|held| (held.slot, held.name.as_str()).cmp(&(slot, name)))
    }
}

impl SimResource for PlayerResources {
    const NAME: &'static str = "mode.player_resources";
}
