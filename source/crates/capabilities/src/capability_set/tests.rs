use std::fs;
use std::path::Path;

use super::*;
use crate::actions::action_book::ActionBook;
use crate::areas::area_effect::AreaEffect;
use crate::capability_set::test_match::TestMatch;
use crate::combat::combat_effect::CombatEffect;
use crate::mode::mode_effect::ModeEffect;
use crate::orders::ai::Ai;
use crate::orders::unit_order::UnitOrder;
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
    TestMatch::new(declared, TestMatch::RATE, budgets).world
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
        UnitOrder::CAPABILITY,
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

/// Visits each source file under `dir` with its production code: the code before the file's
/// first test gate, in every file but `tests.rs`, `bench.rs` and those of a `tests` directory.
fn production(dir: &Path, visit: &mut impl FnMut(&Path, &str)) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if !path.ends_with("tests") {
                production(&path, visit);
            }
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
fn each_capability_installs_after_the_lower_layers_it_builds_on() {
    // The table's install order, its needs and the layers are three facts of one order: a
    // capability builds only on capabilities of lower layers, installed before it.
    let layer = |capability: Capability| {
        let found = LAYERS.iter().find(|(name, _)| *name == capability.name());
        found.map(|&(_, layer)| layer)
    };
    let mut installed = Vec::new();
    for row in CAPABILITIES.iter().filter(|row| row.install.is_some()) {
        let own = layer(row.capability).expect("an installed capability is a module");
        for &need in row.needs {
            assert!(
                installed.contains(&need),
                "{need:?} before {:?}",
                row.capability
            );
            assert!(
                layer(need) < Some(own),
                "{need:?} below {:?}",
                row.capability
            );
        }
        installed.push(row.capability);
    }
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
    assert_eq!(upward, [], "imports from a higher layer");
}

/// Every lookup by name that the production code calls, by file: a method or function whose
/// name ends in `named`, as each lookup of an id by its name is spelled. Each runs in a script
/// call, which resolves the names it is given once, or in the load, which resolves the
/// packages' names once; but the one marked, which runs as a modifier applies. The test fails
/// when a lookup appears and when one listed here is gone.
const LOOKUPS: [(&str, &str); 50] = [
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
    ("mode/mode_call.rs", "param_named"),
    ("mode/mode_schema.rs", "get_named"),
    ("mode/mode_schema.rs", "named"),
    ("progression/progression_api.rs", "track_named"),
    ("progression/tracks_column.rs", "named"),
    ("scripts/ctx.rs", "param_named"),
    ("scripts/call_part.rs", "param_named"),
    ("scripts/frame.rs", "param_named"),
    ("stats/modifier_handle.rs", "field_named"),
    ("stats/param_table.rs", "named"),
    ("stats/stats_api.rs", "modifier_named"),
    ("stats/stats_api.rs", "stat_named"),
    ("stats/stats_call.rs", "named"),
    ("stats/stats_column.rs", "modifier_named"),
    ("stats/stats_column.rs", "named"),
    ("stats/stats_column.rs", "pool_id_named"),
    ("stats/stats_column.rs", "pool_named"),
    ("units/script_view.rs", "damage_kind_named"),
    ("units/script_view.rs", "named"),
    ("units/script_view.rs", "param_named"),
    ("units/script_view.rs", "tag_named"),
    ("units/unit.rs", "param_named"),
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

/// The state types of the core, then those each capability adds to the ones it builds on, in
/// name order: each its own.
const STATE: [(Option<Capability>, &[&str]); 11] = [
    (
        None,
        &[
            "actions.slots",
            "sim.entities",
            "sim.id_allocator",
            "sim.position",
            "sim.tick",
            "units.body",
            "units.owner",
            "units.relations",
            "units.spawn_point",
            "units.team",
            "units.unit_type",
        ],
    ),
    (
        Some(Stats),
        &[
            "stats.level",
            "stats.modifier_clocks",
            "stats.modifiers",
            "stats.player_modifiers",
            "stats.pools",
        ],
    ),
    (
        Some(Capability::Progression),
        &["progression.experience", "progression.level_ups"],
    ),
    (
        Some(Combat),
        &[
            "combat.dead",
            "combat.kept",
            "combat.on_death",
            "combat.recent_attackers",
            "combat.respawn",
        ],
    ),
    (
        Some(Navigation),
        &[
            "navigation.destination",
            "navigation.move_step",
            "navigation.on_path",
            "navigation.path_walker",
            "navigation.progress",
            "navigation.route",
        ],
    ),
    (Some(Vision), &["vision.seen_by", "vision.sight"]),
    (
        Some(Projectiles),
        &["projectiles.projectile", "projectiles.struck_units"],
    ),
    (Some(Capability::Areas), &["areas.area"]),
    (Some(Abilities), &[]),
    (Some(Orders), &["orders.next_think", "orders.resetting"]),
    (Some(Capability::Production), &["production.train_queue"]),
];

#[test]
fn each_capability_adds_exactly_its_own_state_types() {
    fn with_needs(capability: Capability, into: &mut Vec<Capability>) {
        for &need in needs(capability) {
            with_needs(need, into);
        }
        if !into.contains(&capability) {
            into.push(capability);
        }
    }
    for (capability, own) in STATE {
        let mut declared = Vec::new();
        for &need in capability.map_or(&[][..], needs) {
            with_needs(need, &mut declared);
        }
        let below = TestMatch::client(&declared).state_names();
        declared.extend(capability);
        let names = TestMatch::client(&declared).state_names();
        let added: Vec<_> = names
            .iter()
            .copied()
            .filter(|name| !below.contains(name) || capability.is_none())
            .collect();
        assert_eq!(added, own, "{capability:?}");
    }
    // The table holds every capability the release runs.
    let runs = CAPABILITIES.iter().filter(|row| row.install.is_some());
    assert_eq!(runs.count(), STATE.len() - 1);
}
