use std::fs;
use std::path::Path;

use super::*;
use crate::actions::action_book::ActionBook;
use crate::capability_set::test_match::TestMatch;
use crate::orders::ai::Ai;
use crate::scripts::script_limits::ScriptLimits;
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
        per_call: NonZeroU64::MIN,
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
}

/// The capability table of design 04, beside this crate.
const OVERVIEW: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../docs/design/04-capabilities/00-overview.md"
);

#[test]
fn the_design_names_each_capability_and_marks_built_exactly_those_the_release_installs() {
    // Each row of the table names its capabilities in its first column and its status in the
    // second. Every capability but `mode`, which every match has, is in it once.
    let overview = fs::read_to_string(OVERVIEW).unwrap();
    let table = overview
        .lines()
        .skip_while(|line| !line.starts_with("| Capability | Status |"))
        .skip(2)
        .take_while(|line| line.starts_with('|'));
    let (mut named, mut built) = (Vec::new(), Vec::new());
    for row in table {
        let cells: Vec<&str> = row.split('|').map(str::trim).collect();
        let row_names = cells[1].split(", ").map(|name| name.trim_matches('`'));
        named.extend(row_names.clone());
        match cells[2] {
            "built" => built.extend(row_names),
            "planned" => {}
            status => panic!("{status:?} is no status, in {row}"),
        }
    }
    named.sort_unstable();
    let mut every: Vec<&str> = Capability::ALL
        .into_iter()
        .filter(|&capability| capability != Mode)
        .map(Capability::name)
        .collect();
    every.sort_unstable();
    assert_eq!(named, every);
    built.sort_unstable();
    let mut installed: Vec<&str> = CAPABILITIES
        .iter()
        .filter(|row| row.install.is_some())
        .map(|row| row.capability.name())
        .collect();
    installed.sort_unstable();
    assert_eq!(built, installed);
}

/// The layer of each module of the crate, lowest first: a module imports from its own layer
/// and the layers below, as design 02's structural rules ask. `lib.rs` sits above them all.
const LAYERS: [(&str, u8); 21] = [
    ("values", 0),
    ("geometry", 0),
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
    ("items", 5),
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
const LOOKUPS: [(&str, &str); 73] = [
    // The load.
    ("actions/slot_kinds.rs", "named"),
    ("books/book_builder.rs", "cost_target_named"),
    ("books/book_builder.rs", "named"),
    ("books/book_builder.rs", "tag_named"),
    ("combat/combat_bindings.rs", "named"),
    ("combat/combat_rules.rs", "named"),
    ("mode/mode_books.rs", "tag_named"),
    ("mode/mode_data.rs", "named"),
    ("mode/mode_map.rs", "named"),
    ("mode/unit_kit/mod.rs", "named"),
    ("navigation/navigation_rules.rs", "layer_named"),
    ("scripts/script_api/mod.rs", "sorted_named"),
    ("stats/modifier_book.rs", "named"),
    ("stats/pool_book.rs", "named"),
    // The load, and a filter a script names.
    ("units/filter.rs", "tag_named"),
    ("values/filter_data.rs", "named"),
    // Script calls, and the mode inputs, whose names enter with the players' inputs.
    ("abilities/abilities_api.rs", "action_named"),
    ("abilities/abilities_api.rs", "kind_named"),
    ("actions/actions_column.rs", "named"),
    ("combat/combat_api.rs", "damage_kind_named"),
    ("combat/combat_api.rs", "pool_named"),
    ("mode/map_data.rs", "layer_named"),
    ("mode/marker.rs", "named"),
    ("mode/mod.rs", "input_type_named"),
    ("mode/mode_api.rs", "kind_named"),
    ("mode/mode_api.rs", "named"),
    ("mode/mode_api.rs", "path_named"),
    ("mode/mode_api.rs", "resource_named"),
    ("mode/mode_api.rs", "state_field_named"),
    ("mode/mode_api.rs", "unit_type_named"),
    ("mode/mode_call.rs", "param_named"),
    ("mode/mode_schema.rs", "get_named"),
    ("mode/mode_schema.rs", "named"),
    ("mode/mode_setup.rs", "sorted_named"),
    ("mode/roster.rs", "named"),
    ("navigation/paths.rs", "named"),
    ("progression/progression_api.rs", "track_named"),
    ("progression/progression_column.rs", "named"),
    ("scripts/api_builder.rs", "named"),
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
    ("units/script_view.rs", "param_named"),
    ("units/script_view.rs", "path_named"),
    ("units/script_view.rs", "resource_named"),
    ("units/script_view.rs", "tag_named"),
    ("units/script_view.rs", "unit_type_named"),
    ("units/teams.rs", "named"),
    ("units/unit.rs", "param_named"),
    ("units/unit.rs", "tag_named"),
    ("units/unit_state_book.rs", "named"),
    ("units/unit_types.rs", "get_named"),
    ("units/unit_types.rs", "named"),
    ("units/unit_types.rs", "tag_named"),
    ("units/units_column.rs", "field_named"),
    ("units/view_names.rs", "damage_kind_named"),
    ("units/view_names.rs", "named"),
    ("units/view_names.rs", "param_named"),
    ("units/view_names.rs", "tag_named"),
    ("values/name_table.rs", "named"),
    ("values/name_table.rs", "sorted_named"),
    ("values/stat.rs", "named"),
    // As a modifier applies: a param of the ability that applies it, by name, as the place
    // differs by ability.
    ("stats/param_book.rs", "named"),
];

/// Each role a module's type may hold, by the trait it implements or, for a script API, by the
/// `*Api` type that registers members: the suffix of the type's name, and of its file's.
const ROLES: [(&str, &str, &str); 4] = [
    ("impl Effect for ", "Effect", "effect"),
    ("impl ViewColumn for ", "Column", "column"),
    ("impl CallPart for ", "Call", "call"),
    ("struct ", "Api", "api"),
];

/// The types that hold a role but are named for what they are: the orders' effect, which is the
/// order to a unit, and the core's script API, which spans `units`, `scripts` and `players`.
const ROLE_EXCEPTIONS: [&str; 2] = ["UnitOrder", "CoreApi"];

/// The types of `code`, the file `file` of the module `module`, that hold a role and are not
/// named `<Module><Role>` in `<module>_<role>.rs`, as `(file, type)`; a `mod.rs` is named by its
/// directory.
fn misnamed_roles(module: &str, file: &str, code: &str) -> Vec<(String, String)> {
    let pascal: String = module
        .split('_')
        .map(|word| word[..1].to_ascii_uppercase() + &word[1..])
        .collect();
    let mut misnamed = Vec::new();
    for (opening, suffix, file_suffix) in ROLES {
        if suffix == "Api" && !code.contains("fn register(api: &mut ApiBuilder") {
            continue;
        }
        for (at, _) in code.match_indices(opening) {
            if at > 0 && !code[..at].ends_with(|c: char| c.is_whitespace() || c == ')') {
                continue;
            }
            let rest = &code[at + opening.len()..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            let name = &rest[..end];
            if suffix == "Api" && !name.ends_with("Api") {
                continue;
            }
            let named =
                name == format!("{pascal}{suffix}") && file == format!("{module}_{file_suffix}");
            if !named && !ROLE_EXCEPTIONS.contains(&name) {
                misnamed.push((file.to_owned(), name.to_owned()));
            }
        }
    }
    misnamed
}

#[test]
fn each_role_of_a_module_takes_the_modules_name() {
    let src = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
    let mut misnamed = Vec::new();
    let mut held = [0; ROLES.len()];
    production(src, &mut |path, code| {
        let parts: Vec<&str> = path
            .strip_prefix(src)
            .unwrap()
            .iter()
            .map(|part| part.to_str().unwrap())
            .collect();
        let [module, .., _] = parts[..] else {
            return;
        };
        let file = match path.file_stem().unwrap().to_str().unwrap() {
            "mod" => parts[parts.len() - 2],
            stem => stem,
        };
        misnamed.extend(misnamed_roles(module, file, code));
        for (held, (opening, ..)) in held.iter_mut().zip(ROLES) {
            *held += code.matches(opening).count();
        }
    });
    assert_eq!(misnamed, Vec::<(String, String)>::new());
    // The walk reads each role, so a role it no longer finds fails here, not silently.
    assert!(held.iter().all(|&count| count > 0), "{held:?}");

    // Each role is found where it is misnamed, a script API only where it registers members,
    // and the exceptions pass.
    let api = |name: &str| {
        format!("struct {name};\nimpl {name} {{ fn register(api: &mut ApiBuilder<'_>) {{}} }}")
    };
    let samples = [
        (
            "vision",
            "vision_effect",
            "impl Effect for VisionEffect {".to_owned(),
            vec![],
        ),
        (
            "vision",
            "reveal_effect",
            "impl Effect for RevealEffect {".to_owned(),
            vec![("reveal_effect", "RevealEffect")],
        ),
        (
            "vision",
            "vision_column",
            "impl ViewColumn for SightColumn {".to_owned(),
            vec![("vision_column", "SightColumn")],
        ),
        (
            "units",
            "units_call",
            "impl CallPart for UnitsCall {".to_owned(),
            vec![],
        ),
        ("units", "units_api", api("UnitsApi"), vec![]),
        (
            "units",
            "units_api",
            api("PositionApi"),
            vec![("units_api", "PositionApi")],
        ),
        (
            "units",
            "position_api",
            api("UnitsApi"),
            vec![("position_api", "UnitsApi")],
        ),
        ("scripts", "core_api", api("CoreApi"), vec![]),
        (
            "scripts",
            "script_api",
            "pub struct ScriptApi {".to_owned(),
            vec![],
        ),
        (
            "orders",
            "unit_order",
            "impl Effect for UnitOrder {".to_owned(),
            vec![],
        ),
        (
            "capability_set",
            "capability_set",
            "impl Effect for CapabilitySetEffect {".to_owned(),
            vec![("capability_set", "CapabilitySetEffect")],
        ),
    ];
    for (module, file, code, expected) in samples {
        let expected: Vec<(String, String)> = expected
            .into_iter()
            .map(|(file, name): (&str, &str)| (file.to_owned(), name.to_owned()))
            .collect();
        assert_eq!(misnamed_roles(module, file, &code), expected, "{code}");
    }
}

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
const STATE: [(Option<Capability>, &[&str]); 12] = [
    (
        None,
        &[
            "actions.slots",
            "sim.entities",
            "sim.id_allocator",
            "sim.position",
            "sim.tick",
            "units.body",
            "units.dead",
            "units.forced_move",
            "units.lifespan",
            "units.move_step",
            "units.owner",
            "units.relations",
            "units.spawn_point",
            "units.state",
            "units.status_tags",
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
        &[
            "progression.experience",
            "progression.level_ups",
            "progression.points",
        ],
    ),
    (
        Some(Combat),
        &[
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
            "navigation.on_path",
            "navigation.path_walker",
            "navigation.progress",
            "navigation.route",
        ],
    ),
    (
        Some(Vision),
        &["vision.reveals", "vision.seen_by", "vision.sight"],
    ),
    (
        Some(Projectiles),
        &["projectiles.projectile", "projectiles.struck_units"],
    ),
    (Some(Capability::Areas), &["areas.area"]),
    (Some(Abilities), &[]),
    (Some(Orders), &["orders.next_think", "orders.resetting"]),
    (
        Some(Capability::Production),
        &[
            "production.builder",
            "production.gatherer",
            "production.node",
            "production.rally",
            "production.site",
            "production.train_queue",
        ],
    ),
    (Some(Capability::Items), &["items.inventory"]),
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
