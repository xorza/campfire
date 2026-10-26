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

/// A unit's shots in a row of its streak weapons, none before its first and after it stops
/// (04-capabilities/combat.md, A shot streak gives a stage's modifier): the weapon of its last
/// shot, whose stages and fall it counts by, that shot's target, the shots, its stage, and the
/// tick its coast ends. Every unit with slots holds it on a server's match, as Bevy's required
/// component, as a script may grant a streak weapon to any unit; a client never holds it, as the
/// streak is the server's (04-capabilities/combat.md, The streak is the server's).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ShotStreak(Option<Shots>);

/// A streak under way: its count and its target none once it cooled down, as Zero Hour's
/// `FiringTracker::coolDown` sets its count to 0 and forgets its victim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Shots {
    weapon: ActionId,
    target: Option<StableId>,
    count: u32,
    stage: Heat,
    coast_end: Tick,
}

/// Where a streak stands: below its first stage; at a stage, by its index among its weapon's
/// stages; or fallen from one, which its weapon's fall holds until a rise or its next cool-down,
/// as Zero Hour's `CONTINUOUS_FIRE_SLOW` holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Heat {
    Cold,
    Stage(u8),
    Fallen,
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
                if let Some(fall) = streak.and_then(|streak| streak.fall.as_ref()) {
                    appliers.apply(package, fall.modifier.as_str(), Way::NoAction);
                }
            }
        }
        Ok(())
    }

    /// Counts `shot`, of a weapon of `streak`, as Zero Hour's `FiringTracker::shotFired` does:
    /// one more for the target of the shot before, or for another before the coast ends, else
    /// one, a streak past its coast having cooled down; then, at a stage, the streak cools down
    /// when the count is below the stage's shots, and else rises by one when the count passes
    /// the next stage's; below every stage, or fallen, it rises to the first when the count
    /// passes its shots, a fall held only by a weapon of a fall. A stage of another streak weapon
    /// counts as its last stage at most. The first tick past its coast, when it holds a stage or
    /// a fall.
    pub(crate) fn shoot(&mut self, shot: StreakShot, streak: &Streak) -> Option<Tick> {
        let StreakShot {
            weapon,
            target,
            now,
            ready,
        } = shot;
        let before = self.0.filter(|before| now <= before.coast_end);
        let count = match before {
            Some(before) if before.target == Some(target) || now < before.coast_end => {
                before.count.saturating_add(1)
            }
            _ => 1,
        };
        let last = streak.stages.len() - 1;
        let rises = |at: usize| {
            let stage = streak.stages.get(at);
            stage.is_some_and(|stage| count > stage.shots)
        };
        let stage = match before.map_or(Heat::Cold, |before| before.stage) {
            Heat::Stage(at) => {
                let at = usize::from(at).min(last);
                if count < streak.stages[at].shots {
                    None
                } else if rises(at + 1) {
                    Some(Heat::stage(at + 1))
                } else {
                    Some(Heat::stage(at))
                }
            }
            _ if rises(0) => Some(Heat::Stage(0)),
            Heat::Fallen => Some(Heat::cooled(streak)),
            Heat::Cold => Some(Heat::Cold),
        };
        let coast_end = ready.after(streak.coast);
        self.0 = Some(match stage {
            Some(stage) => Shots {
                weapon,
                target: Some(target),
                count,
                stage,
                coast_end,
            },
            None => Shots {
                weapon,
                target: None,
                count: 0,
                stage: Heat::cooled(streak),
                coast_end,
            },
        });
        self.due()
    }

    /// Cools it down in tick `now`, the first past its coast, its last shot's weapon of `streak`,
    /// as Zero Hour's `FiringTracker::update` does: from a stage to the weapon's fall, its coast
    /// ending the fall's hold after `now`, when the weapon has one; else, and from a fall, it
    /// stops. The first tick past the fall's hold, when it falls.
    fn cool(&mut self, now: Tick, streak: Option<&Streak>) -> Option<Tick> {
        let shots = self.0?;
        let fall = streak
            .and_then(|streak| streak.fall)
            .filter(|_| matches!(shots.stage, Heat::Stage(_)));
        self.0 = fall.map(|fall| Shots {
            weapon: shots.weapon,
            target: None,
            count: 0,
            stage: Heat::Fallen,
            coast_end: now.after(fall.hold),
        });
        self.due()
    }

    /// The first tick past its coast, while it holds a stage or a fall.
    fn due(self) -> Option<Tick> {
        let shots = self.0?;
        match shots.stage {
            Heat::Cold => None,
            Heat::Stage(_) | Heat::Fallen => Some(shots.coast_end.after(Ticks::ONE)),
        }
    }

    /// The modifier of its stage or of its fall, of the weapons of `weapons`, while it holds
    /// one.
    fn modifier(self, weapons: &ById<ActionId, Weapon>) -> Option<ModifierId> {
        let shots = self.0?;
        let streak = weapons.get(shots.weapon)?.streak.as_ref()?;
        match shots.stage {
            Heat::Cold => None,
            Heat::Stage(at) => Some(streak.stages[usize::from(at)].modifier),
            Heat::Fallen => Some(streak.fall?.modifier),
        }
    }

    /// Counts each shot this tick's strikes fired of a streak weapon in its unit's streak, in the
    /// order they fired, and pushes the unit at its coast's end while it holds a stage or a fall.
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

    /// Cools down each unit whose coast ended before tick `now`, as Zero Hour's
    /// `FiringTracker::update` does, a unit that falls pushed at its fall's end; a unit whose
    /// streak went on is pushed at its new end.
    pub(crate) fn coast(
        (tick, weapons): (Res<'_, SimTick>, Res<'_, ById<ActionId, Weapon>>),
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
                    let weapon = streak.0.and_then(|shots| weapons.get(shots.weapon));
                    if let Some(at) = streak.cool(now, weapon.and_then(|w| w.streak.as_ref())) {
                        index.push(at, id, entity);
                    }
                }
                None => {}
            }
        }
    }

    /// Pushes the grants of each unit whose streak changed that differ from what it holds under
    /// the streak's hold: its stage's or its fall's modifier from itself at rank 1 while it lives
    /// and holds one, and a release of each other streak modifier it holds. A shot that keeps the
    /// stage pushes none.
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

impl Heat {
    /// The stage at `at` among its weapon's stages, which fit `u8`.
    fn stage(at: usize) -> Self {
        Self::Stage(u8::try_from(at).expect("a streak's stages fit u8"))
    }

    /// Where a streak of a weapon of `streak` stands as it cools down from a stage: fallen, when
    /// the weapon has a fall, else cold.
    const fn cooled(streak: &Streak) -> Self {
        if streak.fall.is_some() {
            Self::Fallen
        } else {
            Self::Cold
        }
    }
}

/// A unit whose streak holds a stage or a fall is due in the first tick past its coast, dead or
/// alive: a death ends its modifier, and its stage or its fall ends with its coast.
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

    // A streak counts by a streak weapon of the book, at a stage it has, or fallen from one by a
    // weapon of a fall; it holds a target exactly while it counts a shot.
    fn check(&self, world: &World, _: Entity) -> bool {
        let Some(shots) = self.0 else {
            return true;
        };
        let streak = world
            .resource::<ById<ActionId, Weapon>>()
            .get(shots.weapon)
            .and_then(|weapon| weapon.streak.as_ref());
        streak.is_some_and(|streak| {
            let stage = match shots.stage {
                Heat::Cold => true,
                Heat::Stage(at) => usize::from(at) < streak.stages.len(),
                Heat::Fallen => streak.fall.is_some(),
            };
            stage && shots.target.is_some() == (shots.count > 0)
        })
    }
}

impl Replication for ShotStreak {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}

#[cfg(test)]
mod tests;
