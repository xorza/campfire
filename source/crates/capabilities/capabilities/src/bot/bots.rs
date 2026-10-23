use campfire_declarations::{Declarations, InstallCx, Module, NoSystems};
use campfire_script::{ApiVersion, Status};
use campfire_sim::Capability;

use crate::bot::bot_api::{BotApi, OnBotThink};
use crate::bot::bot_call::BotCall;

/// The bots' think: its hook, its script API and the params of its calls, which a match that runs
/// scripts binds after every module's, as the think reads through theirs (03-game-scripting.md, A
/// bot is a Rhai script of its game, which the server's bot driver runs).
#[derive(Debug)]
pub(crate) struct Bots;

impl Module for Bots {
    const CAPABILITY: Option<Capability> = None;
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];
    type Systems = NoSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.hook::<OnBotThink>(Status::Runs(ApiVersion::FIRST));
        d.api(BotApi::register);
        d.call_params::<BotCall>();
    }

    /// Adds no system: the bot driver of a host calls the think.
    fn install(_: &mut InstallCx<'_, Self::Systems>) {}
}
