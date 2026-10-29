use std::collections::BTreeMap;
use std::sync::Arc;

use campfire_abilities::Abilities;
use campfire_actions::{ActionData, ActionKeys, ActionTarget, Targeting};
use campfire_areas::{AreaData, AreaDeliveryData, AreaInsideData, Areas};
use campfire_common::DeclaredName;
use campfire_geometry::internals::KernelScene;
use campfire_math::{Num, Vec3};
use campfire_measure::{Kind, Scale};
use campfire_script::ScriptLimits;
use campfire_sim::{Capability, SimWorld, StableId};
use campfire_stats::Stats;
use campfire_units::internals::Spawn;
use campfire_units::{UnitTypeCore, Units};
use campfire_values::{FilterData, PackageIndex, Team, TypeScope};
use criterion::Criterion;

use crate::bench::{Ticked, run_cases};
use crate::fixtures::abilities::AbilitiesFixtures;
use crate::fixtures::stats::StatsFixtures;
use crate::release::r#match::Match;

/// The casters of a scene: a grid of 10 by 10, 8 m apart.
const CASTERS: usize = 100;
/// The ticks between a caster's casts: its cooldown of 1,001 ms is 31 ticks at 30 a second.
const PERIOD: u64 = 31;
/// The ticks of a run: 20 casts of each caster.
const TICKS: u64 = 20 * PERIOD;

const _: () = assert!(
    CASTERS * 10 == KernelScene::UNITS,
    "a scene of a kernel's units"
);

/// What a caster's ability does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cast {
    /// It strikes the enemy it targets for 20 true damage.
    Strike,
    /// It lands a field of 2 m at the point it targets, for 3 s, which holds a modifier on the
    /// enemies inside it and hits each for 5 true damage as it lands.
    Field,
}

/// A scene of casts: `CASTERS` casters of team 0, each with 9 combatants of team 1 of 500 life in
/// a square 1 m apart 3 m east of it, `KernelScene::UNITS` units in all, and each caster ordered to
/// cast at the middle one of its square, unit or point, as its cooldown ends, as a player's or an
/// AI's order would.
#[derive(Debug)]
struct Casts {
    game: Match,
    targets: Vec<(StableId, ActionTarget)>,
}

impl Ticked for Casts {
    fn tick(&mut self, tick: u64) {
        if tick.is_multiple_of(PERIOD) {
            Abilities::casts(&mut self.game, &self.targets);
        } else {
            self.game.step();
        }
    }
}

impl Casts {
    /// The scene of `cast`, before its first tick.
    fn new(cast: Cast) -> Self {
        let declared = [
            Capability::Stats,
            Capability::Combat,
            Capability::Abilities,
            Capability::Areas,
        ];
        let mut game = Abilities::match_of(ScriptLimits::ROOMY, &declared);
        let strike = Abilities::strike();
        let free = ActionData {
            keys: ActionKeys {
                cost: BTreeMap::new(),
                ..strike.keys.clone()
            },
            ..strike
        };
        let (data, source) = match cast {
            Cast::Strike => (
                free,
                r#"fn on_resolve(ctx, caster, target) { ctx.damage(target, "true", 20); }"#,
            ),
            Cast::Field => {
                Stats::load_modifier(
                    &mut game,
                    PackageIndex::MODE,
                    "slow",
                    &Stats::modifier_data(None),
                    None,
                );
                let field = Units::load_type(
                    &mut game,
                    TypeScope::Mode,
                    "field",
                    &UnitTypeCore::default(),
                );
                let area = AreaData {
                    radius: Num::int(2),
                    delay_ms: 0,
                    duration_ms: 3000,
                    affects: Some(FilterData::parse("enemies").unwrap()),
                    inside: AreaInsideData {
                        caster: None,
                        allies: None,
                        enemies: Some(DeclaredName::new("slow").unwrap()),
                    },
                };
                Areas::load_type(&mut game, field, PackageIndex::MODE, &area);
                Units::seal_tags(&mut game, &BTreeMap::new());
                let shot = ActionData {
                    keys: ActionKeys {
                        targeting: Targeting::Point,
                        ..free.keys
                    },
                    delivery: Some(Arc::new(AreaDeliveryData {
                        area: DeclaredName::new("field").unwrap(),
                    })),
                    ..free
                };
                (
                    shot,
                    r#"fn on_hit(ctx, caster, target, hit) { ctx.damage(target, "true", 5); }"#,
                )
            }
        };
        let action = Abilities::load(&mut game, "cast", &data, source);
        let at = |x: i64, z: i64| Vec3::new(Num::int(x), Num::ZERO, Num::int(z));
        let mut targets = Vec::with_capacity(CASTERS);
        for nth in (0..).take(CASTERS) {
            let (x, z) = (8 * (nth % 10) - 40, 8 * (nth / 10) - 40);
            let caster = Spawn::named(&game, "caster", at(x, z))
                .parts(Abilities::caster_parts(action, 1))
                .place(&mut game);
            let mut middle = None;
            for square in 0..9 {
                let position = at(x + 2 + square % 3, z - 1 + square / 3);
                let enemy = Spawn::named(&game, "combatant", position)
                    .team(Team::new(1))
                    .place(&mut game);
                if square == 4 {
                    middle = Some((enemy, position));
                }
            }
            let (enemy, position) = middle.expect("a square has a middle");
            let target = match cast {
                Cast::Strike => ActionTarget::Unit(enemy),
                Cast::Field => ActionTarget::Point(position),
            };
            targets.push((caster, target));
        }
        Self { game, targets }
    }
}

/// A run of each scene of casts, as `run_cases` times it: `integration/abilities/<case>_casts`,
/// 100 strikes a second, each a script's call; and `integration/areas/<case>_fields`, 100 fields
/// landed a second, each holding its modifier on 9 enemies for 3 s, 300 at a time.
pub(super) fn casts(c: &mut Criterion) {
    for (group, scene, cast) in [
        ("abilities", "casts", Cast::Strike),
        ("areas", "fields", Cast::Field),
    ] {
        let mut group = Kind::Integration.group(c, group, Scale::Frame);
        run_cases(&mut group, scene, TICKS, || Casts::new(cast));
        group.finish();
    }
}
