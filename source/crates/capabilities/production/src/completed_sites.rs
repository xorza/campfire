use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_script::{
    Ctx, Hook, HookSpec, ModeAnswer, ModeEvents, PendingCalls, ScriptRole, Unit,
};
use campfire_sim::{SimResource, StableId};
use serde::{Deserialize, Serialize};

/// The buildings whose sites completed, by stable id in each tick, whose `on_site_complete` has
/// yet to run. The Mode stage runs the calls from the front; those whose call found the mode pool
/// spent stay, and run first in a later tick's Mode stage.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedSites(pub PendingCalls<StableId>);

impl SimResource for CompletedSites {
    const NAME: &'static str = "production.completed_sites";

    // A building by any stable id, which the call finds or not.
    fn check(&self, _: &World) -> bool {
        true
    }
}

/// `on_site_complete(ctx, unit)`: the site of the building `unit` completed, which the mode's
/// script answers.
#[derive(Debug)]
pub struct OnSiteComplete;

impl HookSpec for OnSiteComplete {
    const NAME: &'static str = "on_site_complete";
    const ROLE: ScriptRole = ScriptRole::Mode;
    const PARAMS: &'static [&'static str] = &["ctx", "unit"];
    type Args = (Ctx, Unit);
}

/// A site that completed runs `on_site_complete`, from the mode's pool, with its building; one
/// whose building is gone runs no call.
impl ModeEvents for CompletedSites {
    const HOOKS: &'static [Hook] = &[Hook::of::<OnSiteComplete>()];

    /// The buildings join as their sites complete.
    fn arrive(_: &mut World) {}

    fn len(&self) -> usize {
        self.0.len()
    }

    fn partial(&self) -> u8 {
        self.0.partial()
    }

    fn answer(&self, answer: &mut ModeAnswer<'_, '_>) -> usize {
        self.0
            .iter()
            .take_while(|&&id| {
                let args = |ctx: &Ctx| Some((ctx.clone(), ctx.view().unit(id)?));
                answer.call::<OnSiteComplete>(Some(id), args).is_continue()
            })
            .count()
    }

    fn answered(&mut self, count: usize, partial: u8) {
        self.0.answered(count, partial);
    }

    fn append(&mut self, later: &mut Self) {
        self.0.append(&mut later.0);
    }

    fn hold(&self, _: usize, _: &mut World) {}
}
