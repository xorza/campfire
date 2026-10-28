//! The retaliation of `orders` (combat-response.md, A damaged unit calls its idle allies;
//! combat-response.md, A retaliation's target is chased to its leash).

use campfire_actions::{
    ActionProgress, ActionRange, ActionSlots, AttackTarget, ProgressDues, SlotKind, TargetSource,
};
use campfire_capabilities::Match;
use campfire_codec::Binary;
use campfire_combat::internals::TestWeapon;
use campfire_combat::{Combat, DamageCause, OnDeath};
use campfire_common::{Tick, Ticks};
use campfire_math::{Num, Vec3};
use campfire_orders::Orders;
use campfire_script::MatchNames;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Position, StableId};
use campfire_units::internals::Spawn;
use campfire_units::{Destination, Life, UnitTypeCore, Units};
use campfire_values::{FilterData, Rank, Team, UnitType};

use crate::combatant;
use crate::orders::{armed, meter, orders_match};

/// A chase of 1,000 ms: 30 ticks at the tests' 30 a second.
const CHASE_MS: u64 = 1000;

/// A match of the mode's `[retaliation]` of every enemy within 20 m, its allies within 10 m
/// answering for `CHASE_MS`, and its types: one that answers with a weapon of 2 m at every enemy,
/// walking a meter a tick, of `leash` and `reach`; one alike that does not answer; one that
/// answers with a weapon at its allies alone; and an aggressor.
struct Scene {
    game: Match,
    answers: UnitType,
    silent: UnitType,
    unarmed: UnitType,
    aggressor: UnitType,
}

impl Scene {
    fn new([leash, reach]: [i64; 2]) -> Self {
        let mut game = orders_match();
        Orders::load_retaliation(&mut game, "enemies", [Num::int(20), Num::int(10)], CHASE_MS);
        let weapon = combatant(100, 2, 1, 30, 10);
        let answers = armed(&mut game, weapon.clone(), Some(meter()));
        Orders::load_retaliate(&mut game, answers, Num::int(leash), Num::int(reach));
        let silent = armed(&mut game, weapon, Some(meter()));
        let unarmed = Units::load_next_type(&mut game, &UnitTypeCore::default());
        Units::give_type(&mut game, unarmed, Life::full(Num::int(100)));
        Units::give_type(&mut game, unarmed, OnDeath::Despawn);
        Units::give_type(&mut game, unarmed, meter());
        let allies = game
            .resource::<MatchNames>()
            .resolve_filter(&FilterData::parse("allies").unwrap())
            .unwrap();
        let heal = TestWeapon::new(allies, ActionRange::meters(Num::int(2)), Ticks::new(1));
        let heal = Combat::add_weapon(&mut game, heal);
        let slots = ActionSlots::new([(heal, SlotKind::new(0), Rank::new(1))]);
        Units::give_type(&mut game, unarmed, slots);
        Orders::load_retaliate(&mut game, unarmed, Num::int(leash), Num::int(reach));
        let aggressor = armed(&mut game, combatant(1000, 30, 1, 30, 10), None);
        Self {
            game,
            answers,
            silent,
            unarmed,
            aggressor,
        }
    }

    fn spawn(&mut self, unit_type: UnitType, x: i32, team: u8) -> StableId {
        Spawn::of(unit_type, (x, 0, 0))
            .team(Team::new(team))
            .place(&mut self.game)
    }

    fn target(&self, id: StableId) -> Option<StableId> {
        self.game.part::<ActionProgress>(id).attack_target()
    }
}

/// A damaged unit calls each idle ally within 10 m past its body that answers and may hit its
/// aggressor, itself among them: at the origin, struck by an enemy 15 m off, it and an ally at 5 m
/// take the enemy, chased from where they stood for 30 ticks; an ally at 12 m, one busy with
/// another target, one that does not answer and one whose weapon selects allies alone take none.
/// One that dies of its hit still calls, and one struck from past 20 m calls none.
#[test]
fn a_damaged_unit_calls_its_idle_allies_within_the_radius_that_may_hit_its_aggressor() {
    let mut scene = Scene::new([30, 40]);
    let damaged = scene.spawn(scene.answers, 0, 0);
    let near = scene.spawn(scene.answers, 5, 0);
    let far = scene.spawn(scene.answers, 12, 0);
    let busy = Spawn::of(scene.answers, (0, 0, 5)).place(&mut scene.game);
    let silent = Spawn::of(scene.silent, (-5, 0, 0)).place(&mut scene.game);
    let unarmed = Spawn::of(scene.unarmed, (0, 0, -5)).place(&mut scene.game);
    let aggressor = scene.spawn(scene.aggressor, 15, 1);
    let other = scene.spawn(scene.aggressor, 0, 1);
    *scene.game.part_mut::<Position>(other) = Position::from(Vec3::from((0, 0, 50)));
    Combat::attack(&mut scene.game, busy, other);
    // A unit at 100 m that dies of its hit, and its ally at 105 m.
    let doomed = scene.spawn(scene.answers, 100, 0);
    let doomed_ally = scene.spawn(scene.answers, 105, 0);
    let doomed_aggressor = scene.spawn(scene.aggressor, 110, 1);
    // A unit at 200 m struck from 230 m, and its ally at 205 m.
    let distant = scene.spawn(scene.answers, 200, 0);
    let distant_ally = scene.spawn(scene.answers, 205, 0);
    let distant_aggressor = scene.spawn(scene.aggressor, 230, 1);
    let hits = [
        (aggressor, damaged, 10),
        (doomed_aggressor, doomed, 100),
        (distant_aggressor, distant, 10),
    ];
    for (source, target, amount) in hits {
        Combat::damage(
            &mut scene.game,
            Some(source),
            target,
            amount,
            DamageCause::Effect,
        );
    }
    scene.game.step_with(&[]);
    let targets = [
        damaged,
        near,
        far,
        busy,
        silent,
        unarmed,
        doomed_ally,
        distant_ally,
    ]
    .map(|unit| scene.target(unit));
    let expected = [
        Some(aggressor),
        Some(aggressor),
        None,
        Some(other),
        None,
        None,
        Some(doomed_aggressor),
        None,
    ];
    assert_eq!(targets, expected);
    let source = scene.game.part::<ActionProgress>(near).target_source();
    let from = Vec3::from((5, 0, 0));
    let until = Tick::new(30);
    assert_eq!(source, Some(TargetSource::Retaliation { from, until }));
}

/// A retaliation ends at its time, past its reach and past its leash, each apart, and its unit
/// walks back to where it stood; a save and a restore keep its point and its tick. Each unit
/// chases an enemy, a meter a tick: one of a leash of 30 m and a reach of 40 m at an enemy 1 m
/// off, in its weapon's reach, drops it in tick 30; one of a leash of 3 m at an enemy 10 m off drops it as
/// it stands past 3 m, at x = 104 after tick 3, in tick 4; one of a reach of 5 m at an enemy 10 m
/// off drops it in tick 0.
#[test]
fn a_retaliation_ends_at_its_time_its_leash_and_its_reach_and_its_unit_walks_back() {
    let units =
        [([30, 40], 0, 1), ([3, 40], 100, 10), ([30, 5], 200, 10)].map(|(bounds, x, off)| {
            let mut scene = Scene::new(bounds);
            let unit = scene.spawn(scene.answers, x, 0);
            let enemy = scene.spawn(scene.aggressor, x + off, 1);
            let from = Vec3::from((x, 0, 0));
            let target = AttackTarget::retaliated(enemy, from, Tick::new(30));
            ProgressDues::change_unit(&mut scene.game, unit, |progress, dues| {
                progress.set_attack_target(Some(target), dues);
            });
            // A save and a restore keep the retaliation's point and tick.
            let progress = *scene.game.part::<ActionProgress>(unit);
            assert!(Binary::decodes_again(&progress));
            (scene, unit, from)
        });
    let ended = units.map(|(mut scene, unit, from)| {
        let mut ended = None;
        for tick in 0..40 {
            scene.game.step_with(&[]);
            if ended.is_none() && scene.target(unit).is_none() {
                ended = Some(tick);
                let back = scene.game.part::<Destination>(unit).get();
                let at = scene.game.part::<Position>(unit).get();
                assert!(
                    back == Some(from) || at == from,
                    "tick {tick}: {back:?} from {at:?}"
                );
            }
        }
        (ended, scene.game.part::<Position>(unit).get() == from)
    });
    assert_eq!(ended, [(Some(30), true), (Some(4), true), (Some(0), true)]);
}
