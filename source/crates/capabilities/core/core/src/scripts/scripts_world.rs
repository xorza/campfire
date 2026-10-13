use bevy_ecs::world::World;
use campfire_script::ScriptHost;

use crate::scripts::ctx::Ctx;
use crate::scripts::script_batch::ScriptBatch;

/// What the scripts module gives a world, for its own crate.
pub trait ScriptsWorldCrate {
    /// Reads the view of the match's `ctx`, takes the host out of the world, runs `calls`, puts the
    /// host back, and gives what `calls` gave.
    fn script_batch<T>(&mut self, calls: impl FnOnce(&mut ScriptBatch<'_>) -> T) -> T;
}

impl ScriptsWorldCrate for World {
    fn script_batch<T>(&mut self, calls: impl FnOnce(&mut ScriptBatch<'_>) -> T) -> T {
        let ctx = self.non_send::<Ctx>().clone();
        ctx.view().read(self);
        let mut host = self
            .remove_non_send::<ScriptHost>()
            .expect("the core runs scripts");
        let given = calls(&mut ScriptBatch {
            world: self,
            host: &mut host,
            ctx,
        });
        self.insert_non_send(host);
        given
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;

    use crate::units::view::View;

    /// What the scripts module gives a world for the tests and benches of every crate.
    pub trait ScriptsWorldInternals {
        /// Reads the match's units into its script view, as each batch of script calls does.
        fn read_view(&mut self);
    }

    impl ScriptsWorldInternals for World {
        fn read_view(&mut self) {
            let view = self.non_send::<View>().clone();
            view.read(self);
        }
    }
}
