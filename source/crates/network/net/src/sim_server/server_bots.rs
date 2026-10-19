use campfire_bots::{BotNames, BotScript, BotScriptError, OrderScript};
use campfire_common::PlayerSlot;
use campfire_package::ModePackages;

/// The bots a server plays, each script's names resolved in its match: a script for each slot
/// its plan gives a bot, and one for each slot that becomes a bot's after its player left, whose
/// ticks count from then. A bot slot with no script idles.
#[derive(Debug, Clone, Default)]
pub struct ServerBots {
    pub slots: Vec<SlotBot>,
    pub takeover: Option<BotScript>,
}

/// A slot the plan gives a bot, and the script it plays from the start.
#[derive(Debug, Clone)]
pub struct SlotBot {
    pub slot: PlayerSlot,
    pub script: BotScript,
}

/// A slot the plan gives a bot, and the script it plays as its file reads, its names not yet
/// resolved.
#[derive(Debug, Clone)]
pub struct SlotScript {
    pub slot: PlayerSlot,
    pub script: OrderScript,
}

impl ServerBots {
    /// The bots of `slots` and the takeover script `takeover`, each name of their scripts
    /// resolved in the match of `packages`, as the server does before any session opens; an
    /// error for the first name a script holds that the match does not have.
    pub fn new(
        slots: impl IntoIterator<Item = SlotScript>,
        takeover: Option<&OrderScript>,
        packages: &ModePackages,
    ) -> Result<Self, BotScriptError> {
        let books = packages.name_books();
        let names = BotNames::new(&books);
        let slots = slots.into_iter().map(|SlotScript { slot, script }| {
            Ok(SlotBot {
                slot,
                script: BotScript::new(&script, &names)?,
            })
        });
        Ok(Self {
            slots: slots.collect::<Result<_, BotScriptError>>()?,
            takeover: takeover
                .map(|script| BotScript::new(script, &names))
                .transpose()?,
        })
    }
}

impl SlotScript {
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
    use campfire_script::UnknownName;
    use campfire_test_packages::TestMode;
    use campfire_values::{Named as _, UnitType};

    use super::*;

    #[test]
    fn the_start_refuses_the_first_name_a_bots_script_holds_that_the_match_lacks() {
        let packages = TestMode::Lane.packages();
        let script = |units: &str| {
            let text =
                format!("[[order]]\ntick = 0\nunits = \"{units}\"\naction.orders = \"stop\"\n");
            OrderScript::parse(&text).unwrap()
        };
        let bots = |slot: &str, takeover: Option<&str>| {
            let slots = [SlotScript::new(0, script(slot))];
            let takeover = takeover.map(script);
            ServerBots::new(slots, takeover.as_ref(), &packages).map(|bots| bots.slots.len())
        };
        let unknown = |name: &str| {
            Err(BotScriptError::Unknown(UnknownName::new(
                UnitType::NAMES,
                name,
            )))
        };
        assert_eq!(bots("creep", None), Ok(1));
        assert_eq!(bots("creep", Some("tower")), Ok(1));
        assert_eq!(bots("tank", Some("dozer")), unknown("tank"));
        assert_eq!(bots("creep", Some("dozer")), unknown("dozer"));
    }
}
