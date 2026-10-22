use std::sync::Arc;

use campfire_bots::{BotNames, BotScript, OrderScript, ThinkingBot};
use campfire_capabilities::Books;
use campfire_common::{DeclaredName, Seat};
use campfire_package::ModePackages;

use crate::sim_server::server_bots::error::ServerBotsError;

pub(crate) mod error;

/// The bots a server plays, each resolved in its match: what plays each seat its plan gives a
/// bot, and the order script of each seat that becomes a bot's after its player left, whose ticks
/// count from then; and the books of the match, in which a bot's think resolves the names of its
/// orders, which each run of a server shares. A bot seat with nothing to play idles.
#[derive(Debug, Clone)]
pub struct ServerBots {
    pub seats: Vec<SeatBot>,
    pub takeover: Option<BotScript>,
    pub books: Arc<Books>,
}

/// A seat the plan gives a bot, and what plays it from the start.
#[derive(Debug, Clone)]
pub struct SeatBot {
    pub seat: Seat,
    pub play: BotPlay,
}

/// What plays a server's bot: an order script, its names resolved, or a bot of the mode, which
/// thinks (08-sessions.md, A mode names its bots).
#[derive(Debug, Clone)]
pub enum BotPlay {
    Orders(BotScript),
    Thinks(ThinkingBot),
}

/// A seat the plan gives a bot, and what plays it as the command line names it, its names not
/// yet resolved.
#[derive(Debug, Clone)]
pub struct PlannedBot {
    pub seat: Seat,
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
    /// The bots of `seats` and the takeover script `takeover`, each resolved in the match of
    /// `packages`, as the server does before any session opens; an error for the first name a
    /// script holds that the match does not have, or the first bot the mode does not name.
    pub fn new(
        seats: impl IntoIterator<Item = PlannedBot>,
        takeover: Option<&OrderScript>,
        packages: &ModePackages,
    ) -> Result<Self, ServerBotsError> {
        let books = packages.name_books();
        let names = BotNames::new(&books);
        let seats = seats.into_iter().map(|PlannedBot { seat, plan }| {
            let play = match plan {
                BotPlan::Orders(script) => BotPlay::Orders(BotScript::new(&script, &names)?),
                BotPlan::Mode(name) => {
                    let bot = packages.bot(&name).ok_or(ServerBotsError::NoBot(name))?;
                    BotPlay::Thinks(ThinkingBot::new(bot))
                }
            };
            Ok(SeatBot { seat, play })
        });
        let seats = seats.collect::<Result<_, ServerBotsError>>()?;
        let takeover = takeover
            .map(|script| BotScript::new(script, &names))
            .transpose()?;
        Ok(Self {
            seats,
            takeover,
            books: Arc::new(books),
        })
    }
}

impl PlannedBot {
    /// The bot of the seat of index `seat`, which plays the order script `script`.
    pub const fn orders(seat: u32, script: OrderScript) -> Self {
        Self {
            seat: Seat::new(seat),
            plan: BotPlan::Orders(script),
        }
    }

    /// The bot of the seat of index `seat`, which the mode's bot `name` plays.
    pub const fn mode(seat: u32, name: DeclaredName) -> Self {
        Self {
            seat: Seat::new(seat),
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
        let bots = |seat: &str, takeover: Option<&str>| {
            let seats = [PlannedBot::orders(0, script(seat))];
            let takeover = takeover.map(script);
            ServerBots::new(seats, takeover.as_ref(), &packages).map(|bots| bots.seats.len())
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
            Ok(bots.seats.len())
        };
        assert_eq!(mode("walk"), Ok(1));
        let run = DeclaredName::new("run").unwrap();
        assert_eq!(mode("run"), Err(ServerBotsError::NoBot(run)));
    }
}
