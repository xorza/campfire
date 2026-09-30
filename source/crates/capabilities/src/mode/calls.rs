use bevy_ecs::world::World;
use campfire_script::ScriptError;
use campfire_script::rhai::FuncArgs;
use campfire_sim::Tick;

use crate::mode::mode_ctx::{ModeCtx, ModeEffect};
use crate::mode::mode_state::ModeState;
use crate::mode::picks::Picks;
use crate::mode::player_resources::PlayerResources;
use crate::mode::timers::Timers;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_batch::ScriptBatch;

/// The mode calls of one batch, at one time.
#[derive(Debug)]
pub(crate) struct Calls<'a, 'w> {
    pub(crate) batch: &'a mut ScriptBatch<'w>,
    pub(crate) ctx: &'a ModeCtx,
    now: Tick,
}

impl Calls<'_, '_> {
    /// Runs `calls` in one script batch at the time `now`, and gives what `calls` gave.
    pub(crate) fn batch<T>(
        world: &mut World,
        ctx: &ModeCtx,
        now: Tick,
        calls: impl FnOnce(&mut Calls<'_, '_>) -> T,
    ) -> T {
        ScriptBatch::run(world, ctx.view(), |batch| {
            calls(&mut Calls { batch, ctx, now })
        })
    }

    /// Runs `hook` with `args` from `pool`: on success its state and choices commit and its
    /// effects apply; on failure nothing changes.
    pub(crate) fn run(
        &mut self,
        pool: Pool,
        hook: Hook,
        args: impl FuncArgs,
    ) -> Result<(), ScriptError> {
        self.begin();
        let script = self.ctx.book().schema.script;
        drop(self.batch.call(pool, script, hook, args)?);
        self.commit();
        Ok(())
    }

    /// Fills the frame with the mode's state as it stands.
    fn begin(&mut self) {
        let mut frame = self.ctx.frame();
        frame
            .state
            .clone_from(&self.batch.world().resource::<ModeState>().0);
        frame
            .picks
            .clone_from(self.batch.world().resource::<Picks>());
        frame
            .resources
            .clone_from(self.batch.world().resource::<PlayerResources>());
        frame.effects.clear();
    }

    /// Commits the call's state and choices, then applies its effects in order.
    fn commit(&mut self) {
        let mut frame = self.ctx.frame();
        let world = self.batch.world();
        world.resource_mut::<ModeState>().0.clone_from(&frame.state);
        world.resource_mut::<Picks>().clone_from(&frame.picks);
        world
            .resource_mut::<PlayerResources>()
            .clone_from(&frame.resources);
        let book = self.ctx.book();
        for effect in frame.effects.drain(..) {
            match effect {
                ModeEffect::Timer {
                    name,
                    ticks,
                    repeat,
                    data,
                } => world
                    .resource_mut::<Timers>()
                    .set(self.now, name, ticks, repeat, data),
                ModeEffect::SpawnHeroes => book.spawn_heroes(world),
                ModeEffect::SpawnUnit {
                    unit_type,
                    team,
                    pos,
                } => drop(book.spawn(world, unit_type, team, pos, ())),
                ModeEffect::SpawnWave { team, lane, types } => {
                    book.spawn_wave(world, team, lane, &types);
                }
            }
        }
    }
}
