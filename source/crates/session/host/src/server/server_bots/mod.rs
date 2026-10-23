use std::sync::Arc;

use campfire_bots::{BotNames, BotScript, OrderScript, ThinkingBot};
use campfire_capabilities::Books;
use campfire_common::{DeclaredName, PlayerSlot};
use campfire_package::ModePackages;

use crate::server::server_bots::error::ServerBotsError;

pub(crate) mod error;

/// The bots a server plays, each resolved in its match: what plays each player slot its plan gives
/// a bot, and the order script of each player slot that becomes a bot's after its player left,
/// whose ticks count from then; and the books of the match, in which a bot's think resolves the
/// names of its orders, which each run of a server shares. A bot player slot with nothing to play
/// idles.
#[derive(Debug, Clone)]
pub struct ServerBots {
    pub player_slots: Vec<PlayerSlotBot>,
    pub takeover: Option<BotScript>,
    pub books: Arc<Books>,
}

/// A player slot the plan gives a bot, and what plays it from the start.
#[derive(Debug, Clone)]
pub struct PlayerSlotBot {
    pub player_slot: PlayerSlot,
    pub play: BotPlay,
}

/// What plays a server's bot: an order script, its names resolved, or a bot of the mode, which
/// thinks (08-sessions.md, A mode names its bots).
#[derive(Debug, Clone)]
pub enum BotPlay {
    Orders(BotScript),
    Thinks(ThinkingBot),
}

/// A player slot the plan gives a bot, and what plays it as the command line names it, its names
/// not yet resolved.
#[derive(Debug, Clone)]
pub struct PlannedBot {
    pub player_slot: PlayerSlot,
    pub plan: BotPlan,
}

/// What plays a server's bot as the command line names it: an order script as its file reads,
/// or a bot of the mode by its name.
#[derive(Debug, Clone)]
pub enum BotPlan {
    Orders(OrderScript),
    Mode(DeclaredName),
}

impl ServerBots {
    /// The bots of `player_slots` and the takeover script `takeover`, each resolved in the match of
    /// `packages`, as the server does before any session opens; an error for the first name a
    /// script holds that the match does not have, or the first bot the mode does not name.
    pub fn new(
        player_slots: impl IntoIterator<Item = PlannedBot>,
        takeover: Option<&OrderScript>,
        packages: &ModePackages,
    ) -> Result<Self, ServerBotsError> {
        let books = packages.load_books();
        let names = BotNames::new(books);
        let player_slots = player_slots
            .into_iter()
            .map(|PlannedBot { player_slot, plan }| {
                let play = match plan {
                    BotPlan::Orders(script) => BotPlay::Orders(BotScript::new(&script, &names)?),
                    BotPlan::Mode(name) => {
                        let bot = packages.bot(&name).ok_or(ServerBotsError::NoBot(name))?;
                        BotPlay::Thinks(ThinkingBot::new(bot))
                    }
                };
                Ok(PlayerSlotBot { player_slot, play })
            });
        let player_slots = player_slots.collect::<Result<_, ServerBotsError>>()?;
        let takeover = takeover
            .map(|script| BotScript::new(script, &names))
            .transpose()?;
        Ok(Self {
            player_slots,
            takeover,
            books: Arc::clone(books),
        })
    }
}

impl PlannedBot {
    /// The bot of the player slot of index `player_slot`, which plays the order script `script`.
    pub const fn orders(player_slot: u32, script: OrderScript) -> Self {
        Self {
            player_slot: PlayerSlot::new(player_slot),
            plan: BotPlan::Orders(script),
        }
    }

    /// The bot of the player slot of index `player_slot`, which the mode's bot `name` plays.
    pub const fn mode(player_slot: u32, name: DeclaredName) -> Self {
        Self {
            player_slot: PlayerSlot::new(player_slot),
            plan: BotPlan::Mode(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_bots::BotScriptError;
    use campfire_script::UnknownName;
    use campfire_test_packages::TestMode;
    use campfire_values::{Named as _, UnitType};

    use super::*;

    #[test]
    fn the_start_refuses_the_first_name_a_bots_script_holds_or_a_bot_the_mode_lacks() {
        let packages = TestMode::Lane.packages();
        let script = |units: &str| {
            let text =
                format!("[[order]]\ntick = 0\nunits = \"{units}\"\ndirective.orders = \"stop\"\n");
            OrderScript::parse(&text).unwrap()
        };
        let bots = |player_slot: &str, takeover: Option<&str>| {
            let player_slots = [PlannedBot::orders(0, script(player_slot))];
            let takeover = takeover.map(script);
            ServerBots::new(player_slots, takeover.as_ref(), &packages)
                .map(|bots| bots.player_slots.len())
        };
        let unknown = |name: &str| {
            Err(ServerBotsError::Script(BotScriptError::Unknown(
                UnknownName::new(UnitType::NAMES, name),
            )))
        };
        assert_eq!(bots("creep", None), Ok(1));
        assert_eq!(bots("creep", Some("tower")), Ok(1));
        assert_eq!(bots("tank", Some("dozer")), unknown("tank"));
        assert_eq!(bots("creep", Some("dozer")), unknown("dozer"));
        // The lane mode names its bot `walk`, and no `run`.
        let mode = |name: &str| {
            let name = DeclaredName::new(name).unwrap();
            let bots = ServerBots::new([PlannedBot::mode(1, name)], None, &packages)?;
            Ok(bots.player_slots.len())
        };
        assert_eq!(mode("walk"), Ok(1));
        let run = DeclaredName::new("run").unwrap();
        assert_eq!(mode("run"), Err(ServerBotsError::NoBot(run)));
    }
}
