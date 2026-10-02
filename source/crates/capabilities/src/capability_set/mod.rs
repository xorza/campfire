use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_sim::{Capability, StateRegistry};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::abilities::Abilities;
use crate::areas::Areas;
use crate::capability_set::error::CapabilityError;
use crate::combat::Combat;
use crate::mode::Mode;
use crate::mode::match_end::MatchEnd;
use crate::navigation::Navigation;
use crate::orders::Orders;
use crate::production::Production;
use crate::progression::Progression;
use crate::projectiles::Projectiles;
use crate::scripts::ctx::Ctx;
use crate::scripts::effects::ApplyEffect;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::stats::Stats;
use crate::units::Units;
use crate::vision::Vision;

pub(crate) mod error;

/// A mode's declared capabilities: none twice, not `mode`, which every match has, and each with
/// the ones it builds on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilitySet(u16);

/// A capability's install into a match.
type Install = fn(&mut World, &mut Schedule, &mut StateRegistry);

/// One row of `CAPABILITIES`: the capability, how it installs, `None` for one the release does
/// not run yet and for the mode, which installs itself once the others did; the capabilities it
/// builds on; and how it applies the effects a call queues, when its calls queue any.
#[derive(Debug, Clone, Copy)]
struct Row {
    capability: Capability,
    install: Option<Install>,
    needs: &'static [Capability],
    effects: Option<ApplyEffect>,
}

const fn row(capability: Capability, install: Install, needs: &'static [Capability]) -> Row {
    Row {
        capability,
        install: Some(install),
        needs,
        effects: None,
    }
}

const fn planned(capability: Capability) -> Row {
    Row {
        capability,
        install: None,
        needs: &[],
        effects: None,
    }
}

impl Row {
    /// The same row, whose capability applies its effects by `apply`.
    const fn applying(self, apply: ApplyEffect) -> Row {
        Row {
            effects: Some(apply),
            ..self
        }
    }
}

/// Every capability once, in the order they install: each after the ones it builds on. A declared
/// capability the release does not run yet installs nothing.
const CAPABILITIES: [Row; Capability::ALL.len()] = [
    row(Capability::Stats, Stats::install, &[]).applying(Stats::apply_next),
    row(
        Capability::Progression,
        Progression::install,
        &[Capability::Stats],
    )
    .applying(Progression::apply_next),
    row(Capability::Combat, Combat::install, &[Capability::Stats]).applying(Combat::apply_next),
    row(Capability::Navigation, Navigation::install, &[]),
    row(Capability::Vision, Vision::install, &[Capability::Combat]),
    row(
        Capability::Projectiles,
        Projectiles::install,
        &[Capability::Combat],
    )
    .applying(Projectiles::apply_next),
    row(Capability::Areas, Areas::install, &[Capability::Combat]).applying(Areas::apply_next),
    row(
        Capability::Abilities,
        Abilities::install,
        &[Capability::Combat],
    ),
    row(
        Capability::Orders,
        Orders::install,
        &[Capability::Combat, Capability::Navigation],
    )
    .applying(Orders::apply_next),
    row(Capability::Production, Production::install, &[]),
    planned(Capability::Character),
    planned(Capability::Hitscan),
    planned(Capability::Physics),
    planned(Capability::Persistence),
    planned(Capability::Mode).applying(Mode::apply_next),
];

/// How each capability applies the effects a call queues, by capability index: what the frame
/// dispatches each effect by, so the script runtime names no capability.
const DISPATCH: [Option<ApplyEffect>; Capability::ALL.len()] = {
    let mut dispatch: [Option<ApplyEffect>; Capability::ALL.len()] = [None; _];
    let mut at = 0;
    while at < CAPABILITIES.len() {
        let row = CAPABILITIES[at];
        dispatch[row.capability as usize] = row.effects;
        at += 1;
    }
    dispatch
};

const _: () = assert!(
    Capability::ALL.len() <= u16::BITS as usize,
    "a set holds each capability in a bit of its u16"
);

impl CapabilitySet {
    /// The set of `declared`; an error when one is `mode`, one is declared twice, or one lacks a
    /// capability it builds on.
    pub fn new(declared: &[Capability]) -> Result<CapabilitySet, CapabilityError> {
        let mut set = CapabilitySet(0);
        for &capability in declared {
            if capability == Capability::Mode {
                return Err(CapabilityError::DeclaresMode);
            }
            if set.contains(capability) {
                return Err(CapabilityError::Repeated(capability));
            }
            set.0 |= bit(capability);
        }
        for capability in set.iter() {
            if let Some(&needs) = needs(capability)
                .iter()
                .find(|&&needs| !set.contains(needs))
            {
                return Err(CapabilityError::Needs { capability, needs });
            }
        }
        Ok(set)
    }

    pub const fn contains(self, capability: Capability) -> bool {
        self.0 & bit(capability) != 0
    }

    /// The declared capabilities, in the order of their indices.
    pub fn iter(self) -> impl Iterator<Item = Capability> {
        Capability::ALL
            .into_iter()
            .filter(move |&capability| self.contains(capability))
    }

    /// Installs the core, then each declared capability the release runs, each after the ones it
    /// builds on. A match that ended runs no stage. With no script `budgets`, as on a client,
    /// which runs no scripts, the core has no script host, and each capability leaves out what
    /// runs scripts.
    pub fn install(
        self,
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        budgets: Option<ScriptBudgets>,
    ) {
        Units::install(world, schedule, registry, budgets);
        if let Some(ctx) = world.get_non_send::<Ctx>() {
            ctx.frame().set_dispatch(DISPATCH);
        }
        MatchEnd::stop_stages(schedule);
        for row in CAPABILITIES {
            if let Some(install) = row.install
                && self.contains(row.capability)
            {
                install(world, schedule, registry);
            }
        }
    }
}

/// A manifest's `capabilities`, checked as `CapabilitySet::new` checks them.
impl<'de> Deserialize<'de> for CapabilitySet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<CapabilitySet, D::Error> {
        let declared = Vec::<Capability>::deserialize(deserializer)?;
        CapabilitySet::new(&declared).map_err(D::Error::custom)
    }
}

const fn bit(capability: Capability) -> u16 {
    1 << capability as u16
}

/// The capabilities `capability` builds on.
const fn needs(capability: Capability) -> &'static [Capability] {
    let mut at = 0;
    while at < CAPABILITIES.len() {
        if CAPABILITIES[at].capability as u16 == capability as u16 {
            return CAPABILITIES[at].needs;
        }
        at += 1;
    }
    panic!("the table holds every capability")
}

#[cfg(test)]
pub(crate) mod internals {
    use std::fmt;

    use campfire_math::SegmentSeed;
    use campfire_sim::{SimUpdate, TickRate};

    use super::*;
    use crate::combat::internals;
    use crate::stats::pool_id::PoolId;

    /// A match for a capability's tests: a world that `SimUpdate::prepare` set up, with the core
    /// and the declared capabilities installed; and its schedule and state registry, for what a
    /// test installs or loads before the schedule goes into the world.
    pub(crate) struct TestMatch {
        pub(crate) world: World,
        pub(crate) schedule: Schedule,
        pub(crate) registry: StateRegistry,
    }

    /// A schedule prints nothing, so a match prints only what it is.
    impl fmt::Debug for TestMatch {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("TestMatch")
        }
    }

    impl TestMatch {
        /// A match at `rate` of the capabilities `declared`, running scripts within `budgets`;
        /// with combat, its life pool the first.
        pub(crate) fn new(
            declared: &[Capability],
            rate: TickRate,
            budgets: Option<ScriptBudgets>,
        ) -> TestMatch {
            let mut world = World::new();
            SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]), rate);
            let mut schedule = SimUpdate::schedule();
            let mut registry = StateRegistry::new();
            let set = CapabilitySet::new(declared).expect("a test declares a valid set");
            set.install(&mut world, &mut schedule, &mut registry, budgets);
            if set.contains(Capability::Combat) {
                internals::bind_life(&mut world, PoolId::FIRST);
            }
            TestMatch {
                world,
                schedule,
                registry,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::num::NonZeroU32;
    use std::path::Path;

    use campfire_script::ScriptHost;
    use campfire_sim::TickRate;

    use super::*;
    use crate::actions::action_book::ActionBook;
    use crate::areas::area_effect::AreaEffect;
    use crate::capability_set::internals::TestMatch;
    use crate::combat::combat_effect::CombatEffect;
    use crate::mode::mode_effect::ModeEffect;
    use crate::orders::ai::Ai;
    use crate::orders::ai_order::AiOrder;
    use crate::progression::progression_effect::ProgressionEffect;
    use crate::projectiles::projectile_effect::ProjectileEffect;
    use crate::scripts::effects::Effect;
    use crate::scripts::script_limits::ScriptLimits;
    use crate::stats::modifier_effect::ModifierEffect;
    use crate::units::by_type::ByType;
    use crate::units::script_view::View;

    use Capability::{Abilities, Combat, Mode, Navigation, Orders, Projectiles, Stats, Vision};

    #[test]
    fn a_set_holds_each_capability_once_with_what_it_builds_on() {
        let set = CapabilitySet::new(&[Orders, Navigation, Stats, Combat, Vision]).unwrap();
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            [Combat, Stats, Orders, Navigation, Vision]
        );
        assert!(set.contains(Orders) && !set.contains(Abilities));
        assert_eq!(CapabilitySet::new(&[]).unwrap().iter().count(), 0);
        for (declared, error) in [
            (&[Combat, Mode][..], CapabilityError::DeclaresMode),
            (
                &[Combat],
                CapabilityError::Needs {
                    capability: Combat,
                    needs: Stats,
                },
            ),
            (&[Combat, Vision, Combat], CapabilityError::Repeated(Combat)),
            (
                &[Projectiles],
                CapabilityError::Needs {
                    capability: Projectiles,
                    needs: Combat,
                },
            ),
            (
                &[Abilities],
                CapabilityError::Needs {
                    capability: Abilities,
                    needs: Combat,
                },
            ),
            (
                &[Stats, Combat, Orders],
                CapabilityError::Needs {
                    capability: Orders,
                    needs: Navigation,
                },
            ),
            (
                &[Navigation, Orders],
                CapabilityError::Needs {
                    capability: Orders,
                    needs: Combat,
                },
            ),
        ] {
            assert_eq!(CapabilitySet::new(declared), Err(error), "{declared:?}");
        }
    }

    /// Installs `declared` into a fresh match that runs `scripts`.
    fn installed(declared: &[Capability], budgets: Option<ScriptBudgets>) -> World {
        let rate = TickRate::new(NonZeroU32::new(30).unwrap());
        TestMatch::new(declared, rate, budgets).world
    }

    #[test]
    fn a_match_without_scripts_installs_no_host_or_ai_and_the_core_its_actions() {
        let all = [Stats, Combat, Navigation, Projectiles, Abilities, Orders];
        let limits = ScriptLimits {
            per_call: 1,
            player: 1,
            think: 1,
            mode: 1,
        };
        let scripts = ScriptBudgets::new(limits, 1);
        let scripted = installed(&all, Some(scripts.clone()));
        let client = installed(&all, None);
        for (world, scripts) in [(&scripted, true), (&client, false)] {
            assert_eq!(world.get_non_send::<ScriptHost>().is_some(), scripts);
            assert!(world.contains_resource::<ActionBook>());
            assert_eq!(world.contains_resource::<ByType<Ai>>(), scripts);
            assert!(world.get_non_send::<View>().is_some());
        }
        let combat_only = installed(&[Stats, Combat], Some(scripts));
        assert!(combat_only.contains_resource::<ActionBook>());
    }

    #[test]
    fn the_table_holds_every_capability_once_after_what_it_builds_on() {
        for capability in Capability::ALL {
            let rows: Vec<usize> = (0..CAPABILITIES.len())
                .filter(|&at| CAPABILITIES[at].capability == capability)
                .collect();
            assert_eq!(rows.len(), 1, "{capability:?}");
            for &needed in CAPABILITIES[rows[0]].needs {
                let before = CAPABILITIES[..rows[0]]
                    .iter()
                    .any(|row| row.capability == needed);
                assert!(before, "{capability:?} installs before {needed:?}");
            }
        }
        // The capabilities whose calls queue effects apply them, each effect type by the one it
        // names, and no other capability does.
        let applying: Vec<Capability> = Capability::ALL
            .into_iter()
            .filter(|&capability| DISPATCH[capability as usize].is_some())
            .collect();
        let effects = [
            CombatEffect::CAPABILITY,
            ModifierEffect::CAPABILITY,
            ProjectileEffect::CAPABILITY,
            AreaEffect::CAPABILITY,
            AiOrder::CAPABILITY,
            ModeEffect::CAPABILITY,
            ProgressionEffect::CAPABILITY,
        ];
        assert_eq!(applying, effects);
    }

    /// The layer of each module of the crate, lowest first: a module imports from its own layer
    /// and the layers below, as design 02's structural rules ask. `lib.rs` sits above them all.
    const LAYERS: [(&str, u8); 19] = [
        ("values", 0),
        ("units", 1),
        ("scripts", 1),
        ("players", 1),
        ("stats", 2),
        ("actions", 3),
        ("combat", 4),
        ("deliveries", 5),
        ("projectiles", 5),
        ("areas", 5),
        ("abilities", 5),
        ("navigation", 5),
        ("vision", 5),
        ("progression", 5),
        ("production", 5),
        ("orders", 6),
        ("mode", 7),
        ("capability_set", 8),
        ("books", 8),
    ];

    /// The imports from a higher layer that the code holds today, each a module and the one it
    /// imports. Each step of the structural redesign's layers removes its own; the test fails
    /// when a new one appears, and when one listed here is gone, so the list only shrinks.
    const KNOWN_BREAKS: [(&str, &str); 19] = [
        ("actions", "combat"),
        ("actions", "orders"),
        ("combat", "projectiles"),
        ("navigation", "mode"),
        ("production", "mode"),
        ("scripts", "abilities"),
        ("scripts", "actions"),
        ("scripts", "areas"),
        ("scripts", "combat"),
        ("scripts", "mode"),
        ("scripts", "orders"),
        ("scripts", "production"),
        ("scripts", "progression"),
        ("scripts", "projectiles"),
        ("scripts", "stats"),
        ("scripts", "vision"),
        ("units", "actions"),
        ("units", "progression"),
        ("units", "stats"),
    ];

    /// Visits each source file under `dir` with its production code: the code before the file's
    /// first test gate, in every file but `tests.rs` and `bench.rs`.
    fn production(dir: &Path, visit: &mut impl FnMut(&Path, &str)) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                production(&path, visit);
                continue;
            }
            let source = path.extension().is_some_and(|extension| extension == "rs");
            let name = path.file_name().unwrap().to_str().unwrap();
            if !source || name == "tests.rs" || name == "bench.rs" {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap();
            let code = ["#[cfg(test)]", "#[cfg(any(test"]
                .iter()
                .filter_map(|gate| text.find(gate))
                .min()
                .map_or(text.as_str(), |gate| &text[..gate]);
            visit(&path, code);
        }
    }

    /// Each `crate::<module>` the production code of `dir`'s files names, beside the module the
    /// file is in.
    fn imports(dir: &Path, module: &str, found: &mut Vec<(String, String)>) {
        production(dir, &mut |_, code| {
            for (at, _) in code.match_indices("crate::") {
                let rest = &code[at + "crate::".len()..];
                let end = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(rest.len());
                found.push((module.to_owned(), rest[..end].to_owned()));
            }
        });
    }

    #[test]
    fn a_module_imports_only_from_its_layer_and_below() {
        let src = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        let layer = |module: &str| {
            LAYERS
                .iter()
                .find(|(name, _)| *name == module)
                .map(|&(_, layer)| layer)
        };
        let mut found = Vec::new();
        for (module, _) in LAYERS {
            imports(&src.join(module), module, &mut found);
        }
        let mut upward: Vec<(String, String)> = found
            .into_iter()
            .filter(|(from, to)| {
                let to_layer = layer(to);
                assert!(
                    to_layer.is_some() || to.is_empty(),
                    "{from} imports crate::{to}, a module with no layer"
                );
                to_layer > layer(from)
            })
            .collect();
        upward.sort();
        upward.dedup();
        let known: Vec<(String, String)> = KNOWN_BREAKS
            .iter()
            .map(|&(from, to)| (from.to_owned(), to.to_owned()))
            .collect();
        assert_eq!(upward, known, "imports from a higher layer");
    }

    /// Every lookup by name that the production code calls, by file: a method or function whose
    /// name ends in `named`, as each lookup of an id by its name is spelled. Each runs in a script
    /// call, which resolves the names it is given once, or in the load, which resolves the
    /// packages' names once; but the one marked, which runs as a modifier applies. The test fails
    /// when a lookup appears and when one listed here is gone.
    const LOOKUPS: [(&str, &str); 45] = [
        // The load.
        ("actions/slot_kinds.rs", "named"),
        ("books/book_builder.rs", "cost_target_named"),
        ("books/book_builder.rs", "named"),
        ("combat/combat_bindings.rs", "named"),
        ("combat/combat_rules.rs", "named"),
        ("mode/mode_books.rs", "tag_named"),
        ("mode/mode_data.rs", "named"),
        ("mode/mode_map.rs", "named"),
        ("mode/unit_kit/mod.rs", "named"),
        ("navigation/navigation_rules.rs", "layer_named"),
        ("stats/modifier_book.rs", "named"),
        ("stats/pool_book.rs", "named"),
        // The load, and a filter a script names.
        ("units/filter.rs", "tag_named"),
        ("values/filter_data.rs", "named"),
        // Script calls, and the mode inputs, whose names enter with the players' inputs.
        ("combat/combat_api.rs", "damage_kind_named"),
        ("combat/combat_api.rs", "pool_named"),
        ("mode/mod.rs", "input_type_named"),
        ("mode/mode_api.rs", "named"),
        ("mode/mode_api.rs", "path_named"),
        ("mode/mode_api.rs", "resource_named"),
        ("mode/mode_api.rs", "state_field_named"),
        ("mode/mode_api.rs", "unit_type_named"),
        ("mode/mode_schema.rs", "get_named"),
        ("mode/mode_schema.rs", "named"),
        ("progression/progression_api.rs", "track_named"),
        ("scripts/ctx.rs", "param_named"),
        ("scripts/frame.rs", "named"),
        ("stats/modifier_handle.rs", "field_named"),
        ("stats/param_table.rs", "named"),
        ("stats/stats_api.rs", "modifier_named"),
        ("units/script_view.rs", "damage_kind_named"),
        ("units/script_view.rs", "modifier_named"),
        ("units/script_view.rs", "named"),
        ("units/script_view.rs", "param_named"),
        ("units/script_view.rs", "pool_id_named"),
        ("units/script_view.rs", "tag_named"),
        ("units/unit.rs", "param_named"),
        ("units/unit.rs", "pool_named"),
        ("units/unit.rs", "stat_named"),
        ("units/unit.rs", "tag_named"),
        ("units/unit_types.rs", "get_named"),
        ("units/unit_types.rs", "tag_named"),
        ("values/name_table.rs", "named"),
        ("values/stat.rs", "named"),
        // As a modifier applies: a param of the ability that applies it, by name, as the place
        // differs by ability.
        ("stats/param_book.rs", "named"),
    ];

    #[test]
    fn a_name_is_looked_up_only_by_a_script_call_or_the_load() {
        let src = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        let mut found = Vec::new();
        production(src, &mut |path, code| {
            let parts = path.strip_prefix(src).unwrap().iter();
            let parts: Vec<&str> = parts.map(|part| part.to_str().unwrap()).collect();
            let file = parts.join("/");
            for line in code.lines() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for (at, _) in line.match_indices("named(") {
                    let start = line[..at]
                        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                        .map_or(0, |before| before + 1);
                    let lookup = &line[start..at + "named".len()];
                    if !line[..start].trim_end().ends_with("fn") {
                        found.push((file.clone(), lookup.to_owned()));
                    }
                }
            }
        });
        found.sort();
        found.dedup();
        let mut known: Vec<(String, String)> = LOOKUPS
            .iter()
            .map(|&(file, lookup)| (file.to_owned(), lookup.to_owned()))
            .collect();
        known.sort();
        assert_eq!(found, known, "lookups by name");
    }
}
