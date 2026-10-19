use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_script::{
    Ctx, Hook, HookDecl, ModeAnswer, ModeEvents, PendingCalls, ScriptRole, ScriptText, Unit,
};
use campfire_sim::{SimResource, StableId};
use campfire_values::{NameId, PathId};
use serde::{Deserialize, Serialize};

use crate::navigation_book::NavigationBook;

/// The walkers that reached the last waypoint of their paths, in the order they reached it, whose
/// `on_path_end` has yet to run. The Mode stage runs the calls from the front; those whose call
/// found the mode pool spent stay, and run first in a later tick's Mode stage.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathArrivals(pub PendingCalls<PathArrival>);

/// A walker that reached the last waypoint of its path, `path`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathArrival {
    pub id: StableId,
    pub path: PathId,
}

impl SimResource for PathArrivals {
    const NAME: &'static str = "navigation.path_arrivals";

    // A path the map lacks has no name for `on_path_end` to read.
    fn check(&self, world: &World) -> bool {
        let count = world.resource::<NavigationBook>().paths.count() as usize;
        self.0.iter().all(|arrival| arrival.path.index() < count)
    }
}

/// `on_path_end(ctx, unit, path)`: `unit` reached the last waypoint of the path named `path`, from
/// the end it walked from, which the mode's script answers.
#[derive(Debug)]
pub struct OnPathEnd;

impl HookDecl for OnPathEnd {
    const NAME: &'static str = "on_path_end";
    const ROLE: ScriptRole = ScriptRole::Mode;
    const PARAMS: &'static [&'static str] = &["ctx", "unit", "path"];
    type Args = (Ctx, Unit, ScriptText);
}

/// A walker that reached its path's last waypoint runs `on_path_end`, from the mode's pool, with
/// the unit and the path's name; one whose unit is gone runs no call.
impl ModeEvents for PathArrivals {
    const HOOKS: &'static [Hook] = &[Hook::of::<OnPathEnd>()];

    /// The walkers join as they reach the last waypoint.
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
            .take_while(|&&PathArrival { id, path }| {
                let args = |ctx: &Ctx| {
                    let view = ctx.view();
                    let unit = view.unit(id)?;
                    let name = ScriptText::from(view.names().name(path));
                    Some((ctx.clone(), unit, name))
                };
                answer.call::<OnPathEnd>(Some(id), args).is_continue()
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
