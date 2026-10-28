use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_declarations::ById;
use campfire_math::Num;
use campfire_players::PlayersWorld;
use campfire_script::{
    Ctx, Hook, HookDecl, ModeAnswer, ModeEvents, PendingCalls, ScriptRole, ScriptText, Unit,
};
use campfire_sim::{SimResource, StableId};
use campfire_stats::{Level, PlayerModifiers};
use campfire_units::{LevelRaised, Owner};
use campfire_values::{NameId, TrackId, UnitType};
use serde::{Deserialize, Serialize};

use crate::experience::Experience;
use crate::player_experience::PlayerExperience;
use crate::player_points::PlayerPoints;
use crate::points::Points;
use crate::track_book::TrackBook;
use crate::type_tracks::{TrackStart, TypeTracks};

/// The levels units reached, in the order reached, whose `on_level_up` has yet to run. The Mode
/// stage runs the calls from the front; the level-ups whose call found the mode budget spent stay,
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
        let types = world.get_resource::<ById<UnitType, TypeTracks>>().cloned();
        let tracks = types
            .as_ref()
            .zip(world.get::<UnitType>(entity))
            .and_then(|(types, &unit_type)| types.get(unit_type));
        let thresholds = tracks.map_or_else(
            || book.thresholds(track),
            |tracks| tracks.thresholds(track, &book),
        );
        let raised = world
            .get_mut::<Experience>(entity)
            .expect("a unit given experience has tracks")
            .add(track, amount, thresholds);
        if raised.to == raised.from {
            return;
        }
        if book.level_track() == Some(track) {
            // A track whose levels give no point gives its units none to hold.
            let gained = book.points_to(track, raised.to) - book.points_to(track, raised.from);
            if gained > 0 {
                world
                    .get_mut::<Points>(entity)
                    .expect("a unit of a `level` track of points has points")
                    .gain(gained);
            }
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

impl LevelUps {
    /// Gives the player of `player` `amount` of experience on `track`, one of the player tracks it
    /// holds: its level rises to what the experience reaches, and its points by those the levels
    /// reached give (04-capabilities/progression.md, A track of players). A player's level-up runs
    /// no `on_level_up`, a unit's hook.
    pub(crate) fn grant_player(world: &mut World, player: Entity, track: TrackId, amount: Num) {
        let book = world.resource::<TrackBook>().clone();
        let raised = world
            .get_mut::<PlayerExperience>(player)
            .expect("a player of a mode of player tracks holds its experience")
            .0
            .add(track, amount, book.thresholds(track));
        let gained = book.points_to(track, raised.to) - book.points_to(track, raised.from);
        if gained > 0 {
            world
                .get_mut::<PlayerPoints>(player)
                .expect("a player of a track of points has points")
                .gain(gained);
        }
    }

    /// Gives the unit of `entity`, which the mode's spawner spawned, the experience of the highest
    /// of its type's start levels on each track that holds, always or by a modifier its player
    /// holds, as `grant` raises a level: the spawn callback of progression
    /// (04-capabilities/progression.md, A start level by a modifier).
    pub(crate) fn start(world: &mut World, entity: Entity) {
        let unit = world.entity(entity);
        let (Some(&unit_type), Some(&id)) = (unit.get::<UnitType>(), unit.get::<StableId>()) else {
            return;
        };
        let owner = unit.get::<Owner>().map(|owner| owner.player_slot());
        let Some(types) = world.get_resource::<ById<UnitType, TypeTracks>>() else {
            return;
        };
        let Some(tracks) = types
            .get(unit_type)
            .filter(|tracks| !tracks.starts().is_empty())
        else {
            return;
        };
        let tracks = tracks.clone();
        let held = owner
            .and_then(|owner| world.player_entity(owner))
            .and_then(|player| world.get::<PlayerModifiers>(player));
        let book = world.resource::<TrackBook>().clone();
        // The highest level of each track's starts that hold, as one grant reaches it.
        let mut highest = [None::<Level>; TrackId::LIMIT];
        let holds = |start: &&TrackStart| {
            start
                .requires
                .is_none_or(|requires| held.is_some_and(|held| held.holds(requires)))
        };
        for start in tracks.starts().iter().filter(holds) {
            let at = &mut highest[start.track.index()];
            *at = Some(at.map_or(start.level, |level| level.max(start.level)));
        }
        let mut gains = [None; TrackId::LIMIT];
        for (at, level) in highest.into_iter().enumerate() {
            let Some(level) = level else {
                continue;
            };
            let track = TrackId::at(at);
            let xp = world
                .get::<Experience>(entity)
                .and_then(|experience| experience.get(track))
                .map_or(Num::ZERO, |held| held.xp);
            let needed = tracks.thresholds(track, &book).of_level(level);
            gains[at] = needed
                .filter(|&needed| needed > xp)
                .map(|needed| (track, needed - xp));
        }
        for (track, amount) in gains.into_iter().flatten() {
            Self::grant(world, entity, id, track, amount);
        }
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

/// A level a unit reached runs `on_level_up`, from the mode's budget, with the unit, the track's
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
