use bevy_ecs::world::World;
use campfire_script::rhai::FuncArgs;
use campfire_script::{Budget, ScriptError, ScriptHost};

use crate::mode::mode_ctx::{ModeCtx, ModeEffect};
use crate::mode::mode_state::ModeState;
use crate::mode::picks::Picks;
use crate::mode::player_resources::PlayerResources;
use crate::mode::timers::Timers;
use crate::units::error::CallError;
use crate::units::hook::Hook;
use crate::units::script_failures::{ScriptFailure, ScriptFailures};

/// The mode calls of one batch, which share the view as the batch began, the host and a pool.
#[derive(Debug)]
pub(crate) struct Calls<'a> {
    pub(crate) world: &'a mut World,
    host: &'a mut ScriptHost,
    budget: &'a mut Budget,
    pub(crate) ctx: &'a ModeCtx,
    now: u64,
}

impl Calls<'_> {
    /// Reads the view, takes the host, runs `calls`, puts the host back, and gives what `calls`
    /// gave.
    pub(crate) fn batch<T>(
        world: &mut World,
        ctx: &ModeCtx,
        budget: &mut Budget,
        now: u64,
        calls: impl FnOnce(&mut Calls<'_>) -> T,
    ) -> T {
        ctx.view().read(world);
        let mut host = world
            .remove_non_send::<ScriptHost>()
            .expect("units are installed");
        let given = calls(&mut Calls {
            world,
            host: &mut host,
            budget,
            ctx,
            now,
        });
        world.insert_non_send(host);
        given
    }

    /// Runs `hook` with `args`, the call at the batch's time: on success its state and choices
    /// commit and its effects apply; on failure nothing changes.
    pub(crate) fn run(&mut self, hook: Hook, args: impl FuncArgs) -> Result<(), ScriptError> {
        self.begin();
        let script = self.ctx.book().script;
        drop(self.host.call(self.budget, script, hook.name(), args)?);
        self.commit();
        Ok(())
    }

    /// Records that a call of `hook` failed with `error`.
    pub(crate) fn record(&mut self, hook: Hook, error: ScriptError) {
        self.world
            .non_send_mut::<ScriptFailures>()
            .0
            .push(ScriptFailure {
                unit: None,
                hook,
                error: CallError::from_script(error),
            });
    }

    /// Fills the frame with the mode's state as it stands.
    fn begin(&mut self) {
        let mut frame = self.ctx.frame();
        frame
            .state
            .clone_from(&self.world.resource::<ModeState>().0);
        frame.picks.clone_from(self.world.resource::<Picks>());
        frame
            .resources
            .clone_from(self.world.resource::<PlayerResources>());
        frame.effects.clear();
    }

    /// Commits the call's state and choices, then applies its effects in order.
    fn commit(&mut self) {
        let mut frame = self.ctx.frame();
        let world = &mut *self.world;
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
