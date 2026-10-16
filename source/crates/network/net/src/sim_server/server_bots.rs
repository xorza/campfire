use campfire_common::PlayerSlot;
use campfire_values::{Names, UnitType};

use crate::bot_script::BotScript;
use crate::order_script::OrderScript;
use crate::order_script::error::UnknownUnitType;

/// The bots a server plays: a script for each slot its plan gives a bot, and one for each slot
/// that becomes a bot's after its player left, whose ticks count from then. A bot slot with no
/// script idles.
#[derive(Debug, Clone, Default)]
pub struct ServerBots {
    pub slots: Vec<SlotBot>,
    pub takeover: Option<OrderScript>,
}

/// A slot the plan gives a bot, and the script it plays from the start.
#[derive(Debug, Clone)]
pub struct SlotBot {
    pub slot: PlayerSlot,
    pub script: OrderScript,
}

impl ServerBots {
    /// An error for the first unit type a script names that `types`, the mode's, lacks, as the
    /// server checks before any session opens.
    pub fn check(&self, types: &Names<UnitType>) -> Result<(), UnknownUnitType> {
        let slots = self.slots.iter().map(|bot| &bot.script);
        for script in slots.chain(&self.takeover) {
            BotScript::new(script, types)?;
        }
        Ok(())
    }
}

impl SlotBot {
    /// The bot of the slot of index `slot`, which plays `script`.
    pub const fn new(slot: u32, script: OrderScript) -> Self {
        Self {
            slot: PlayerSlot::new(slot),
            script,
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_values::{DeclaredName, TypeScope};

    use super::*;

    #[test]
    fn the_start_refuses_the_first_unit_type_a_bots_script_names_that_the_mode_lacks() {
        let types: Names<UnitType> = [(TypeScope::Mode, "ranger")].into();
        let script = |units: &str| {
            let text = format!("[[order]]\ntick = 0\nunits = \"{units}\"\nmove = [1, 1]\n");
            OrderScript::parse(&text).unwrap()
        };
        let bots = |slot: &str, takeover: Option<&str>| ServerBots {
            slots: vec![SlotBot::new(0, script(slot))],
            takeover: takeover.map(script),
        };
        let unknown = |name| {
            Err(UnknownUnitType {
                name: DeclaredName::new(name).unwrap(),
            })
        };
        assert_eq!(bots("ranger", None).check(&types), Ok(()));
        assert_eq!(bots("ranger", Some("ranger")).check(&types), Ok(()));
        assert_eq!(bots("tank", Some("dozer")).check(&types), unknown("tank"));
        assert_eq!(
            bots("ranger", Some("dozer")).check(&types),
            unknown("dozer")
        );
    }
}
