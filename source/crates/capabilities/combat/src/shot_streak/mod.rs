use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Changed, Has};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_actions::ActionsData;
use campfire_common::{Tick, Ticks};
use campfire_declarations::{BookCx, BookError, ById};
use campfire_sim::{
    DueIndex, DueUnit, SimComponent, SimTick, StableId, TimedKept, TimedProcess, TimedRuns,
};
use campfire_states::{NotSent, Replication};
use campfire_stats::{Grant, GrantFrom, Grants, Hold, ModifierAppliers, Modifiers, Way};
use campfire_units::{Dead, UnitEntity};
use campfire_values::{ActionId, ModifierId, Rank};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::attack_data::AttackData;
use crate::streak_shots::{FiredShot, StreakShot, StreakShots};
use crate::weapon::{Streak, Weapon};

/// A unit's shots in a row of its streak weapons, none before its first
/// (04-capabilities/combat.md, A shot streak gives a stage's modifier): the weapon of its last
/// shot, whose stages it counts by, that shot's target, the shots, its stage, and the tick its
/// coast ends. Every unit with slots holds it on a server's match, as Bevy's required component,
/// as a script may grant a streak weapon to any unit; a client never holds it, as the streak is
/// the server's (04-capabilities/combat.md, The streak is the server's).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ShotStreak(Option<Shots>);

/// A streak under way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Shots {
    weapon: ActionId,
    target: StableId,
    count: u32,
    /// The index of its stage among its weapon's stages; none below the first, or past its
    /// coast.
    stage: Option<u8>,
    coast_end: Tick,
}

impl ShotStreak {
    /// Adds that each package's streak weapons apply the modifiers of their stages, with no
    /// action, as a unit's streak grants them, read from the actions' data, as the appliers come
    /// before the action book; an error for a modifier its package does not have comes with the
    /// weapons' book.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn applied_ways(
        cx: &BookCx<'_>,
        appliers: &mut ModifierAppliers,
    ) -> Result<(), BookError> {
        for (package, source) in cx.packages() {
            let actions = source.content.get::<ActionsData>().into_iter();
            for (_, action) in actions.flat_map(|actions| &actions.0) {
                let streak = action
                    .section::<AttackData>()
                    .and_then(|data| data.streak.as_ref());
                for stage in streak.into_iter().flat_map(|streak| &streak.stages) {
                    appliers.apply(package, stage.modifier.as_str(), Way::NoAction);
                }
            }
        }
        Ok(())
    }

    /// Counts `shot`, of a weapon of `streak`: one more for the target of the shot before, or
    /// for another before the coast ends, else one; then the stage falls to none when the count
    /// is below its own stage's shots, or rises by one when it passes the next stage's, as Zero
    /// Hour's `FiringTracker::shotFired` steps. A stage of another streak weapon counts as its last
    /// stage at most. The tick the stage ends, when it holds one.
    pub(crate) fn shoot(&mut self, shot: StreakShot, streak: &Streak) -> Option<Tick> {
        let StreakShot {
            weapon,
            target,
            now,
            ready,
        } = shot;
        let count = match self.0 {
            Some(before) if before.target == target || now < before.coast_end => {
                before.count.saturating_add(1)
            }
            _ => 1,
        };
        let last = streak.stages.len() - 1;
        let held = self
            .0
            .and_then(|before| before.stage)
            .map(|stage| usize::from(stage).min(last));
        let stage = match held {
            Some(at) if count < streak.stages[at].shots => None,
            held => {
                let next = held.map_or(0, |at| at + 1);
                let rises = streak
                    .stages
                    .get(next)
                    .is_some_and(|stage| count > stage.shots);
                if rises { Some(next) } else { held }
            }
        };
        self.0 = Some(Shots {
            weapon,
            target,
            count,
            stage: stage.map(|at| u8::try_from(at).expect("a streak's stages fit u8")),
            coast_end: ready.after(streak.coast),
        });
        self.due()
    }

    /// The first tick past its coast, while it holds a stage.
    fn due(self) -> Option<Tick> {
        let shots = self.0?;
        shots.stage?;
        Some(shots.coast_end.after(Ticks::ONE))
    }

    /// The modifier of its stage, of the weapons of `weapons`, while it holds one.
    fn modifier(self, weapons: &ById<ActionId, Weapon>) -> Option<ModifierId> {
        let shots = self.0?;
        let stage = shots.stage?;
        let streak = weapons.get(shots.weapon)?.streak.as_ref()?;
        Some(streak.stages[usize::from(stage)].modifier)
    }

    /// Counts each shot this tick's strikes fired of a streak weapon in its unit's streak, in the
    /// order they fired, and pushes the unit at its coast's end while it holds a stage.
    pub(crate) fn count(
        (weapons, mut fired): (Res<'_, ById<ActionId, Weapon>>, ResMut<'_, StreakShots>),
        (mut index, mut shots): (ResMut<'_, DueIndex<Self>>, Local<'_, Vec<FiredShot>>),
        mut streaks: Query<'_, '_, &mut Self>,
    ) {
        fired.take(&mut shots);
        for &FiredShot { unit, entity, shot } in &*shots {
            let streak = weapons
                .get(shot.weapon)
                .and_then(|weapon| weapon.streak.as_ref())
                .expect("a listed shot is of a streak weapon");
            let mut counted = streaks
                .get_mut(entity)
                .expect("a server's unit with slots holds its streak");
            if let Some(due) = counted.shoot(shot, streak) {
                index.push(due, unit, entity);
            }
        }
    }

    /// Ends the stage of each unit whose coast ended before tick `now`, its count kept, as Zero
    /// Hour's `FiringTracker::update` cools down past its coast; a unit whose streak went on is
    /// pushed at its new end.
    pub(crate) fn coast(
        tick: Res<'_, SimTick>,
        mut index: ResMut<'_, DueIndex<Self>>,
        mut due: Local<'_, Vec<DueUnit>>,
        mut streaks: Query<'_, '_, &mut Self>,
    ) {
        let now = tick.start();
        due.clear();
        index.take_due(now, &mut due);
        for &DueUnit { id, entity } in &*due {
            let Ok(mut streak) = streaks.get_mut(entity) else {
                continue;
            };
            match streak.due() {
                Some(at) if at > now => index.push(at, id, entity),
                Some(_) => {
                    if let Some(shots) = &mut streak.0 {
                        shots.stage = None;
                    }
                }
                None => {}
            }
        }
    }

    /// Pushes the grants of each unit whose streak changed that differ from what it holds under
    /// the streak's hold: its stage's modifier from itself at rank 1 while it lives and holds a
    /// stage, and a release of each other streak modifier it holds. A shot that keeps the stage
    /// pushes none.
    pub(crate) fn want(
        weapons: Res<'_, ById<ActionId, Weapon>>,
        mut grants: ResMut<'_, Grants>,
        units: Query<
            '_,
            '_,
            (Entity, &StableId, &Self, Has<Dead>, Option<&Modifiers>),
            Changed<Self>,
        >,
    ) {
        for (entity, &id, &streak, dead, modifiers) in &units {
            let wanted = streak.modifier(&weapons).filter(|_| !dead);
            let streaks = modifiers
                .into_iter()
                .flat_map(Modifiers::iter)
                .filter(|carried| {
                    let instance = carried.instance;
                    instance.source == Some(id) && instance.lifetime.held_by(Hold::Streak)
                });
            let grant = |modifier, rank| Grant {
                modifier,
                from: GrantFrom::Itself,
                hold: Hold::Streak,
                action: None,
                rank,
            };
            let mut run = grants.of(entity, id);
            let mut holds_wanted = false;
            for carried in streaks {
                let modifier = carried.instance.id;
                if Some(modifier) == wanted {
                    holds_wanted = true;
                } else {
                    run.push(grant(modifier, None));
                }
            }
            if let Some(modifier) = wanted.filter(|_| !holds_wanted) {
                run.push(grant(modifier, Some(Rank::FIRST)));
            }
        }
    }
}

/// A unit whose streak holds a stage is due in the first tick past its coast, dead or alive: a
/// death ends its modifier, and its stage ends with its coast.
impl TimedProcess for ShotStreak {
    const RUNS: TimedRuns = TimedRuns::Server;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = &'static Self;
    type Books = ();

    fn due((): &(), streak: &Self) -> Option<Tick> {
        streak.due()
    }
}

impl SimComponent for ShotStreak {
    const NAME: &'static str = "combat.shot_streak";
    type Host = UnitEntity;

    // A streak counts by a streak weapon of the book, at a stage it has.
    fn check(&self, world: &World, _: Entity) -> bool {
        let Some(shots) = self.0 else {
            return true;
        };
        let streak = world
            .resource::<ById<ActionId, Weapon>>()
            .get(shots.weapon)
            .and_then(|weapon| weapon.streak.as_ref());
        streak.is_some_and(|streak| {
            shots.count > 0
                && shots
                    .stage
                    .is_none_or(|stage| usize::from(stage) < streak.stages.len())
        })
    }
}

impl Replication for ShotStreak {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}

#[cfg(test)]
mod tests;
