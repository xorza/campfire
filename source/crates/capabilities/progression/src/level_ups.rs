use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_script::{
    Ctx, Hook, HookDecl, ModeAnswer, ModeEvents, PendingCalls, ScriptRole, ScriptText, Unit,
};
use campfire_sim::{SimResource, StableId};
use campfire_stats::Level;
use campfire_units::LevelRaised;
use campfire_values::TrackId;
use serde::{Deserialize, Serialize};

use crate::experience::Experience;
use crate::points::Points;
use crate::track_book::TrackBook;

/// The levels units reached, in the order reached, whose `on_level_up` has yet to run. The Mode
/// stage runs the calls from the front; the level-ups whose call found the mode pool spent stay,
/// and run first in a later tick's Mode stage.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelUps(pub PendingCalls<LevelUp>);

/// A level a unit reached on a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelUp {
    pub id: StableId,
    pub track: TrackId,
    pub level: Level,
}

impl LevelUps {
    /// Gives the unit `id`, of `entity`, `amount` of experience on `track`, which it has: its level
    /// rises to what the experience reaches, and each level reached joins the tick's level-ups. A
    /// level reached on the `level` track gives the unit a point for each level, and raises the
    /// unit's level, which stats owns.
    pub(crate) fn grant(
        world: &mut World,
        entity: Entity,
        id: StableId,
        track: TrackId,
        amount: Num,
    ) {
        let book = world.resource::<TrackBook>().clone();
        let raised = world
            .get_mut::<Experience>(entity)
            .expect("a unit given experience has tracks")
            .add(track, amount, &book);
        if raised.to == raised.from {
            return;
        }
        if book.level_track() == Some(track) {
            world
                .get_mut::<Points>(entity)
                .expect("a unit with the `level` track has points")
                .gain(raised.to.get() - raised.from.get());
            world.trigger(LevelRaised {
                entity,
                level: raised.to.into(),
            });
        }
        let reached = (raised.from.get() + 1..=raised.to.get())
            .map(|level| Level::new(level).expect("a level past another"));
        let mut level_ups = world.resource_mut::<Self>();
        level_ups
            .0
            .extend(reached.map(|level| LevelUp { id, track, level }));
    }
}

impl SimResource for LevelUps {
    const NAME: &'static str = "progression.level_ups";

    // A track the mode lacks has no name for `on_level_up` to read.
    fn check(&self, world: &World) -> bool {
        let book = world.resource::<TrackBook>();
        self.0
            .iter()
            .all(|level_up| book.names().has(level_up.track))
    }
}

/// `on_level_up(ctx, unit, track, level)`: a unit reached `level` on `track`, which the mode's
/// script answers.
#[derive(Debug)]
pub struct OnLevelUp;

impl HookDecl for OnLevelUp {
    const NAME: &'static str = "on_level_up";
    const ROLE: ScriptRole = ScriptRole::Mode;
    const PARAMS: &'static [&'static str] = &["ctx", "unit", "track", "level"];
    type Args = (Ctx, Unit, ScriptText, i64);
}

/// A level a unit reached runs `on_level_up`, from the mode's pool, with the unit, the track's
/// name and the level; one whose unit is gone runs no call. A level that a call reaches joins the
/// end, so a chain of level-ups ends within the tick, as levels are finite.
impl ModeEvents for LevelUps {
    const HOOKS: &'static [Hook] = &[Hook::of::<OnLevelUp>()];

    /// The levels join as they are reached.
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
            .take_while(|&&LevelUp { id, track, level }| {
                let args = |ctx: &Ctx| {
                    let view = ctx.view();
                    let unit = view.unit(id)?;
                    let name = ScriptText::from(view.names().name(track));
                    Some((ctx.clone(), unit, name, i64::from(level.get())))
                };
                answer.call::<OnLevelUp>(Some(id), args).is_continue()
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
