use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Tick};
use campfire_core::{ScriptsWorldCrate, StateValue, SyncAll, SyncNone, Team, UnitsWorld};

use crate::calls::Calls;
use crate::match_end::MatchEnd;
use crate::match_mode::MatchMode;
use crate::mode_state::{ModeState, ModeSync};
use crate::save_asked::SaveAsked;

/// What the mode module gives a world, for every crate.
pub trait ModeWorld {
    /// The end of the match, once it ended.
    fn match_end(&self) -> Option<MatchEnd>;

    /// The boundary the match's mode last asked for a save at, by `ctx.save()`, by the tick
    /// it comes before.
    fn save_asked(&self) -> Option<Tick>;

    /// The mode's state in the match, every field in the order of their names, as a
    /// mode call reads it.
    fn mode_state(&self) -> Vec<StateValue>;

    /// The team of player `slot` in the match; `None` before the mode installs, or for
    /// a slot the session does not have.
    fn team_of(&self, slot: PlayerSlot) -> Option<Team>;
}

impl ModeWorld for World {
    fn match_end(&self) -> Option<MatchEnd> {
        self.get(self.match_entity()?).copied()
    }

    fn save_asked(&self) -> Option<Tick> {
        self.get_resource::<SaveAsked>().map(|asked| asked.at())
    }

    fn mode_state(&self) -> Vec<StateValue> {
        let mut state = Vec::new();
        let (none, all) = (
            self.mode_fields::<SyncNone>(),
            self.mode_fields::<SyncAll>(),
        );
        self.non_send::<MatchMode>()
            .schema
            .merge_state(none, all, &mut state);
        state
    }

    fn team_of(&self, slot: PlayerSlot) -> Option<Team> {
        self.get_non_send::<MatchMode>()?.teams.of(slot)
    }
}

/// What the mode module gives a world, for its own crate.
pub(crate) trait ModeWorldCrate {
    /// The fields of the mode's state class of sync `S`, on the match's entity, which every
    /// match with a mode spawns with them.
    fn mode_fields<S: ModeSync>(&self) -> &[StateValue];

    /// Writes `values`, the fields of the class of sync `S` as a call left them, unless they are
    /// what the match holds, so a call that changed none marks nothing changed.
    fn commit_mode_fields<S: ModeSync>(&mut self, values: &[StateValue]);

    /// Runs `calls` in one script batch at the time `now`, and gives what `calls` gave.
    fn mode_calls<T>(&mut self, now: Tick, calls: impl FnOnce(&mut Calls<'_, '_>) -> T) -> T;
}

impl ModeWorldCrate for World {
    fn mode_fields<S: ModeSync>(&self) -> &[StateValue] {
        let entity = self
            .match_entity()
            .expect("a match that started has its entity");
        let state = self.get::<ModeState<S>>(entity);
        state
            .expect("the match's entity holds the mode's state")
            .values()
    }

    fn commit_mode_fields<S: ModeSync>(&mut self, values: &[StateValue]) {
        let entity = self
            .match_entity()
            .expect("a match that started has its entity");
        let mut state = self
            .get_mut::<ModeState<S>>(entity)
            .expect("the match's entity holds the mode's state");
        if state.values() != values {
            state.values_mut().clone_from_slice(values);
        }
    }

    fn mode_calls<T>(&mut self, now: Tick, calls: impl FnOnce(&mut Calls<'_, '_>) -> T) -> T {
        self.script_batch(|batch| {
            let ctx = batch.ctx().clone();
            calls(&mut Calls { batch, ctx, now })
        })
    }
}
