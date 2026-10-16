use std::any::type_name;
use std::fmt::Debug;
use std::num::NonZeroU64;
use std::path::Path;

use Capability::{
    Abilities, Combat, Hitboxes, Items, Mode, Navigation, Orders, Production, Progression,
    Projectiles, Quests, Stats, Vision,
};
use bevy_ecs::component::Component;
use bevy_ecs::schedule::Schedules;
use bevy_ecs::world::Mut;
use campfire_actions::ActionBook;
use campfire_common::SegmentSeed;
use campfire_declare::{ByType, EntityParts, Module, Role};
use campfire_math::Vec3;
use campfire_orders::Ai;
use campfire_players::PlayerResources;
use campfire_script::{ScriptBudgets, ScriptLimits, View};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{SimUpdate, StateRegistry};
use campfire_store::internals::SourceFiles;
use campfire_store::{DirEntries, EntryKind, InputFile};
use campfire_units::{Player, UnitTypeCore};
use campfire_values::Team;

use super::*;
use crate::release::r#match::Match;

#[test]
fn a_set_holds_each_capability_once_with_what_it_builds_on() {
    let set = Release::capability_set(&[Orders, Navigation, Stats, Combat, Vision]).unwrap();
    assert_eq!(
        set.iter().collect::<Vec<_>>(),
        [Combat, Stats, Orders, Navigation, Vision]
    );
    assert!(set.contains(Orders) && !set.contains(Abilities));
    assert_eq!(Release::capability_set(&[]).unwrap().iter().count(), 0);
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
        // A capability the design plans, of no module this release builds.
        (&[Stats, Quests], CapabilityError::NotBuilt(Quests)),
        (&[Hitboxes], CapabilityError::NotBuilt(Hitboxes)),
        (
            &[Projectiles],
            CapabilityError::Needs {
                capability: Projectiles,
                needs: Stats,
            },
        ),
        (
            &[Abilities],
            CapabilityError::Needs {
                capability: Abilities,
                needs: Stats,
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
                needs: Stats,
            },
        ),
    ] {
        assert_eq!(
            Release::capability_set(declared),
            Err(error),
            "{declared:?}"
        );
    }
}

/// Installs `declared` into a fresh match that runs `scripts`.
fn installed(declared: &[Capability], budgets: Option<ScriptBudgets>) -> Match {
    Match::test(declared, Match::TEST_RATE, budgets)
}

#[test]
fn a_match_without_scripts_installs_no_host_or_ai_and_the_core_its_actions_and_players() {
    let all = [Stats, Combat, Navigation, Projectiles, Abilities, Orders];
    let limits = ScriptLimits {
        per_call: NonZeroU64::MIN,
        player: 1,
        think: 1,
        mode: 1,
    };
    let scripts = ScriptBudgets::new(limits, 1);
    let mut scripted = installed(&all, Some(scripts.clone()));
    let client = installed(&all, None);
    for (world, scripts) in [(&scripted, true), (&client, false)] {
        assert_eq!(world.get_non_send::<ScriptHost>().is_some(), scripts);
        assert!(world.contains_resource::<ActionBook>());
        assert_eq!(world.contains_resource::<ByType<Ai>>(), scripts);
        assert!(world.get_non_send::<View>().is_some());
        // With no mode, the match holds the players' state, and a player its parts.
        let mut types = Vec::new();
        world
            .resource::<StateRegistry>()
            .hash_by_type(world, &mut types);
        let names: Vec<&str> = types.iter().map(|ty| ty.name).collect();
        for name in [
            "players.held_out",
            "players.match",
            "players.out_of_play",
            "players.player",
            "players.resources",
        ] {
            assert!(names.contains(&name), "{name}");
        }
    }
    {
        let mut player = scripted.spawn_empty();
        EntityParts::<Player>::give(&mut player);
        assert_eq!(
            player.get::<PlayerResources>(),
            Some(&PlayerResources::new(0))
        );
    }
    let combat_only = installed(&[Stats, Combat], Some(scripts));
    assert!(combat_only.contains_resource::<ActionBook>());
}

#[test]
fn the_list_holds_each_capability_at_most_once_after_what_it_builds_on() {
    let rows = Release::rows().0;
    for (at, row) in rows.iter().enumerate() {
        let before = &rows[..at];
        assert!(
            before
                .iter()
                .all(|other| other.capability != row.capability),
            "{:?} is listed once",
            row.capability
        );
        for &needed in row.needs {
            let built = before.iter().any(|other| other.capability == needed);
            assert!(built, "{:?} installs after {needed:?}", row.capability);
        }
    }
    // The mode installs above the list.
    assert!(rows.iter().all(|row| row.capability != Mode));
}

/// The capability table of design 04, beside this crate.
/// The most bytes a test reads of a design document: far past any the design holds.
const DOC_LEN: usize = 1 << 20;

const OVERVIEW: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../docs/design/04-capabilities/00-overview.md"
);

#[test]
fn the_design_names_each_capability_and_marks_built_exactly_those_the_release_installs() {
    // Each row of the table names its capabilities in its first column and its status in the
    // second. Every capability but `mode`, which every match has, is in it once.
    let overview = InputFile::read_text(Path::new(OVERVIEW), DOC_LEN).unwrap();
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
    let mut installed: Vec<&str> = Release::rows()
        .0
        .iter()
        .map(|row| row.capability.name())
        .collect();
    installed.sort_unstable();
    assert_eq!(built, installed);
}

/// Each capability crate's folder under this crate's and its layer, lowest first: a crate
/// depends only on crates of lower layers, and a capability needs only capabilities of lower
/// layers, as design 02's structural rules ask. This crate sits above them all.
const LAYERS: [(&str, u8); 23] = [
    ("core/geometry", 0),
    ("core/state-types", 0),
    ("core/values", 1),
    ("core/visibility", 1),
    ("core/script", 2),
    ("core/declare", 3),
    ("core/units", 4),
    ("core/players", 5),
    ("stats", 6),
    ("vision", 6),
    ("core/actions", 7),
    ("combat", 8),
    ("core/deliveries", 8),
    ("interaction", 8),
    ("items", 8),
    ("progression", 8),
    ("abilities", 9),
    ("areas", 9),
    ("navigation", 9),
    ("projectiles", 9),
    ("production", 10),
    ("mode", 11),
    ("orders", 11),
];

/// The layer of the capability crate `name`, by the last part of its folder.
fn layer(name: &str) -> Option<u8> {
    let found = LAYERS
        .iter()
        .find(|(folder, _)| folder.rsplit('/').next() == Some(name));
    found.map(|&(_, layer)| layer)
}

const MANIFEST_LEN: usize = 1 << 16;

/// The `campfire-<name>` crates among the dependencies of the crate in `folder` that `LAYERS`
/// lists, by `<name>`, in the manifest's order.
fn layered_dependencies(folder: &str) -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root.join(folder).join("Cargo.toml");
    let manifest = InputFile::read_text(&path, MANIFEST_LEN).unwrap();
    let dependencies = manifest
        .split("\n[")
        .find_map(|section| section.strip_prefix("dependencies]\n"))
        .expect("a capability crate has dependencies");
    dependencies
        .lines()
        .filter_map(|line| line.strip_prefix("campfire-"))
        .map(|name| &name[..name.find(['.', ' ']).unwrap()])
        .filter(|name| layer(name).is_some())
        .map(str::to_owned)
        .collect()
}

#[test]
fn each_capability_crate_is_listed_one_layer_above_its_highest_dependency() {
    // Every crate under this one, a folder with a manifest, has its layer, and each layer is
    // one above its highest dependency's, 0 for none: so no crate depends on one of its own
    // layer or above, and no layer is higher than its dependencies make it.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut crates = Vec::new();
    for parent in ["", "core/"] {
        for entry in DirEntries::read(&root.join(parent)).unwrap() {
            let folder = format!("{parent}{}", entry.name.to_str().unwrap());
            let manifest = root.join(&folder).join("Cargo.toml");
            if entry.kind == EntryKind::Dir
                && InputFile::read_if_present(&manifest, MANIFEST_LEN)
                    .unwrap()
                    .is_some()
            {
                crates.push(folder);
            }
        }
    }
    crates.sort_unstable();
    let mut listed: Vec<&str> = LAYERS.iter().map(|&(folder, _)| folder).collect();
    listed.sort_unstable();
    assert_eq!(crates, listed);
    assert!(layered_dependencies("mode").contains(&"stats".to_owned()));
    let misplaced: Vec<String> = LAYERS
        .iter()
        .filter_map(|&(folder, own)| {
            let derived = layered_dependencies(folder)
                .iter()
                .map(|name| layer(name).expect("a layered dependency") + 1)
                .max()
                .unwrap_or(0);
            (derived != own)
                .then(|| format!("{folder}: {own}, where its dependencies give {derived}"))
        })
        .collect();
    assert_eq!(misplaced, Vec::<String>::new());
}

/// Each capability of the release's list, with what it needs and what it uses: the mode's as
/// its scripted part states them, which the mode's core, a part of its crate, builds on none of.
#[derive(Debug, Default)]
struct Edges(Vec<(Capability, &'static [Capability], &'static [Capability])>);

impl ModuleVisitor for Edges {
    fn module<M: Module>(&mut self) {
        if let Some(capability) = M::CAPABILITY.filter(|&capability| capability != Mode) {
            self.0.push((capability, M::NEEDS, M::USES));
        }
    }

    fn scripted<M: Module>(&mut self) {
        let capability = M::CAPABILITY.expect("the scripted part is the mode's");
        self.0.push((capability, M::NEEDS, M::USES));
    }
}

#[test]
fn a_capabilitys_crate_depends_on_exactly_what_it_needs_or_uses() {
    let mut rows = Edges::default();
    Release::modules(&mut rows);
    let mut found = Vec::new();
    for (capability, needs, uses) in rows.0 {
        let name = capability.name();
        let folder = LAYERS
            .iter()
            .map(|&(folder, _)| folder)
            .find(|folder| folder.rsplit('/').next() == Some(name))
            .expect("each capability of the release is a crate of `LAYERS`");
        let mut depends: Vec<String> = layered_dependencies(folder)
            .into_iter()
            .filter(|dependency| {
                Capability::ALL
                    .iter()
                    .any(|other| other.name() == dependency)
            })
            .collect();
        depends.sort();
        let mut edges: Vec<String> = needs
            .iter()
            .chain(uses)
            .map(|edge| edge.name().to_owned())
            .collect();
        edges.sort();
        let twice = needs.iter().any(|need| uses.contains(need));
        if depends != edges || twice {
            found.push(format!(
                "{name}: depends on {depends:?}, needs and uses {edges:?}"
            ));
        }
    }
    assert_eq!(found, Vec::<String>::new());
}

/// Visits each source file under `dir` with its production code, by its path from `dir`.
fn production(dir: &Path, visit: &mut impl FnMut(&str, &str)) {
    for file in SourceFiles::under(dir) {
        if let Some(code) = file.production() {
            visit(&file.path, &code);
        }
    }
}

/// Visits each source file of every capability crate and of this one with its production code,
/// by its path as `<crate>/<path>`, under the crate's folder name; this crate's under their
/// modules' own.
fn capability_sources(visit: &mut impl FnMut(&str, &str)) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for folder in LAYERS.iter().map(|&(folder, _)| folder).chain([""]) {
        let module = match folder.rsplit('/').next().unwrap() {
            "" => None,
            name => Some(name.replace('-', "_")),
        };
        production(
            &root.join(folder).join("src"),
            &mut |path, code| match &module {
                Some(module) => visit(&format!("{module}/{path}"), code),
                None => visit(path, code),
            },
        );
    }
}

#[test]
fn each_capability_installs_after_the_lower_layers_it_builds_on() {
    // The table's install order, its needs and the layers are three facts of one order: a
    // capability builds only on capabilities of lower layers, installed before it.
    let mut installed = Vec::new();
    for row in Release::rows().0 {
        let own = layer(row.capability.name()).expect("an installed capability is a crate");
        for &need in row.needs {
            assert!(
                installed.contains(&need),
                "{need:?} before {:?}",
                row.capability
            );
            assert!(
                layer(need.name()) < Some(own),
                "{need:?} below {:?}",
                row.capability
            );
        }
        installed.push(row.capability);
    }
}

#[test]
fn no_module_names_another_modules_sets() {
    // A set's order is its phase's alone: a module that named another's set would order against
    // it, or put a system in it, outside the one list of phases.
    let module_of = |path: &str| path.split('/').next().unwrap_or_default().to_owned();
    let mut sets = Vec::new();
    capability_sources(&mut |path, code| {
        for (at, _) in code.match_indices("impl ModuleSet for ") {
            let rest = &code[at + "impl ModuleSet for ".len()..];
            let end = rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap();
            sets.push((module_of(path), rest[..end].to_owned()));
        }
    });
    assert!(
        sets.iter()
            .any(|(module, set)| module == "actions" && set == "ActionsSet")
    );
    let mut named = Vec::new();
    capability_sources(&mut |path, code| {
        let module = module_of(path);
        let words = code.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'));
        for word in words {
            let other = sets
                .iter()
                .find(|(owner, set)| set == word && *owner != module);
            if let Some((owner, set)) = other {
                named.push(format!("{path} names {owner}'s {set}"));
            }
        }
    });
    named.sort();
    named.dedup();
    assert_eq!(named, Vec::<String>::new());
}

/// Every lookup by name that the production code of the capabilities and the network calls, by
/// file: a method or function whose name ends in `named`, as each lookup of an id by its name is
/// spelled, a table's `named` the most. Each runs in a script call, which resolves the names it
/// is given once, as its arguments' types take them, in the load, which resolves the packages'
/// names once, or as a bot starts. The test fails when a
/// lookup appears and when one listed here is gone.
const LOOKUPS: [(&str, &str); 44] = [
    // The load.
    ("actions/cost_target.rs", "named"),
    ("actions/slot_kinds.rs", "named"),
    ("books/book_builder.rs", "named"),
    ("books/unit_start/mod.rs", "named"),
    ("combat/armor.rs", "named"),
    ("combat/mode_combat_data.rs", "named"),
    ("mode/game_map.rs", "named"),
    ("mode/map_data.rs", "named"),
    ("mode/mode_data.rs", "named"),
    ("mode/mode_map.rs", "named"),
    ("mode/mode_names.rs", "named"),
    ("stats/modifier_book.rs", "named"),
    ("stats/modifier_spec.rs", "named"),
    ("stats/param_book.rs", "named"),
    ("stats/stat_book.rs", "named"),
    ("units/unit_types.rs", "named"),
    ("values/block.rs", "deserialize_named"),
    ("values/stat.rs", "named"),
    // The load, and the host's settings as the server starts.
    ("visibility/mode_visibility.rs", "named"),
    ("visibility/setting/mod.rs", "named"),
    // A bot's script, each unit type it names resolved once as its bot starts.
    ("net/bot_script/mod.rs", "named"),
    // The load, and a filter a script names.
    ("script/scripts/view/filter.rs", "named"),
    ("values/filter_data.rs", "named"),
    // Script calls, each argument by its type, and the mode inputs, whose names enter with the
    // players' inputs.
    ("actions/slot_kind.rs", "named"),
    ("mode/choice_id.rs", "named"),
    ("mode/marker_tag.rs", "named"),
    ("mode/mode_api.rs", "loadout_ability_named"),
    ("mode/mode_api.rs", "offer_named"),
    ("mode/mode_call.rs", "param_named"),
    ("mode/mode_hooks.rs", "input_type_named"),
    ("mode/mode_schema.rs", "named"),
    ("mode/roster.rs", "named"),
    ("mode/roster.rs", "offer_named"),
    ("script/scripts/id_args.rs", "named"),
    ("script/scripts/script_api/api_builder.rs", "named"),
    ("script/scripts/script_names.rs", "named"),
    ("script/scripts/state_decl/state_schema.rs", "named"),
    ("script/scripts/view/tag_names.rs", "named"),
    ("script/scripts/view/view_names.rs", "named"),
    ("stats/param_table.rs", "named"),
    ("stats/pool_id.rs", "named"),
    ("stats/stat_id.rs", "named"),
    ("stats/stats_call.rs", "named"),
    ("values/name_table.rs", "named"),
];

/// Each role a module's type may hold, by the trait it implements or, for a script API, by the
/// `*Api` type that registers members: the suffix of the type's name, and of its file's.
const ROLES: [(&str, &str, &str); 4] = [
    ("impl Effect for ", "Effect", "effect"),
    ("impl ViewColumn for ", "Column", "column"),
    ("impl CallPart for ", "Call", "call"),
    ("struct ", "Api", "api"),
];

/// The types that hold a role but are named for what they are: the core's script API, which spans
/// `units`, `scripts` and `players`.
const ROLE_EXCEPTIONS: [&str; 1] = ["CoreApi"];

/// The types of `code`, the file `file` of the module `module`, that hold a role and are not
/// named `<Module><Role>` in `<module>_<role>.rs`, as `(file, type)`; a `mod.rs` is named by its
/// directory, and a crate's `lib.rs` by its module.
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
    let mut misnamed = Vec::new();
    let mut held = [0; ROLES.len()];
    capability_sources(&mut |path, code| {
        let parts: Vec<&str> = path.split('/').collect();
        let [module, .., last] = parts[..] else {
            return;
        };
        let file = match last.strip_suffix(".rs").expect("a source ends in .rs") {
            "mod" => parts[parts.len() - 2],
            "lib" => module,
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
            vec![("unit_order", "UnitOrder")],
        ),
        (
            "orders",
            "orders_effect",
            "impl Effect for OrdersEffect {".to_owned(),
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

/// Each role whose types take a suffix, wherever they are: the trait a type implements to hold
/// it, and the suffix of its name. A `*Book` is exactly a book, a `*Data` a table of package data.
const SUFFIX_ROLES: [(&str, &str); 2] = [("Book", "Book"), ("DataSection", "Data")];

/// The types that hold a suffix role but are named for what they are: the one generic book, a
/// book of a value for each unit type.
const SUFFIX_EXCEPTIONS: [&str; 1] = ["ByType"];

/// The types `code` implements `role` for, by name: each `impl … <role> for <type>`.
fn implementers<'c>(code: &'c str, role: &str) -> Vec<&'c str> {
    let opening = format!(" {role} for ");
    let mut found = Vec::new();
    for (at, _) in code.match_indices(&opening) {
        let line = code[..at].rsplit('\n').next().unwrap_or_default();
        if !line.trim_start().starts_with("impl") {
            continue;
        }
        let rest = &code[at + opening.len()..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        found.push(&rest[..end]);
    }
    found
}

/// The types `code` declares whose name ends in `suffix`.
fn suffixed<'c>(code: &'c str, suffix: &str) -> Vec<&'c str> {
    let mut found = Vec::new();
    for (at, _) in code.match_indices("struct ") {
        let rest = &code[at + "struct ".len()..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        if rest[..end].ends_with(suffix) {
            found.push(&rest[..end]);
        }
    }
    found
}

#[test]
fn a_book_and_a_data_table_take_their_suffix() {
    let mut holders: [Vec<String>; SUFFIX_ROLES.len()] = Default::default();
    let mut books = Vec::new();
    capability_sources(&mut |_, code| {
        for (holders, (role, _)) in holders.iter_mut().zip(SUFFIX_ROLES) {
            holders.extend(implementers(code, role).into_iter().map(str::to_owned));
        }
        books.extend(suffixed(code, "Book").into_iter().map(str::to_owned));
    });
    let mut misnamed = Vec::new();
    for (holders, (_, suffix)) in holders.iter().zip(SUFFIX_ROLES) {
        assert!(
            !holders.is_empty(),
            "the walk finds the holders of each role"
        );
        let wrong = holders
            .iter()
            .filter(|name| !name.ends_with(suffix) && !SUFFIX_EXCEPTIONS.contains(&name.as_str()));
        misnamed.extend(wrong.cloned());
    }
    assert_eq!(
        misnamed,
        Vec::<String>::new(),
        "a holder of a role misnamed"
    );
    // Every `*Book` is a book.
    let no_books: Vec<_> = books
        .iter()
        .filter(|name| !holders[0].contains(name))
        .collect();
    assert_eq!(no_books, Vec::<&String>::new(), "a `*Book` that is no book");

    // A generic impl and a plain one are found, and a mention in a comment is not.
    let code =
        "impl<T: PerType> Book for ByType<T> {}\nimpl Book for ActionBook {}\n// a Book for all";
    assert_eq!(implementers(code, "Book"), ["ByType", "ActionBook"]);
    let code = "pub(crate) struct ModeBooks;\nstruct StatBook(u8);";
    assert_eq!(suffixed(code, "Book"), ["StatBook"]);
}

#[test]
fn a_name_is_looked_up_only_by_a_script_call_or_the_load() {
    let mut found = Vec::new();
    let mut visit = |path: &str, code: &str| {
        let file = path.to_owned();
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
    };
    capability_sources(&mut visit);
    // The network's crate, whose bots name unit types.
    let net = Path::new(env!("CARGO_MANIFEST_DIR")).join("../network/net/src");
    production(&net, &mut |path, code| visit(&format!("net/{path}"), code));
    found.sort();
    found.dedup();
    let mut known: Vec<(String, String)> = LOOKUPS
        .iter()
        .map(|&(file, lookup)| (file.to_owned(), lookup.to_owned()))
        .collect();
    known.sort();
    assert_eq!(found, known, "lookups by name");
}

#[test]
fn a_unit_holds_only_parts_the_declarations_list() {
    // The mode's module, which the capability of the same name hides here.
    use campfire_mode::Mode;
    // A match of every capability the release runs: a unit holds only parts the declarations
    // list, as state or derived; one more is named.
    /// A part of a unit that no declaration lists.
    #[derive(Component, Debug)]
    struct Unlisted;
    let set = Release::capabilities();
    let declared: Vec<Capability> = set.iter().collect();
    let mut game = Match::unscripted(&declared);
    let unit_type = Units::load_next_type(&mut game, &UnitTypeCore::default());
    let id = Mode::spawn(&mut game, unit_type, Vec3::ZERO, Team::new(0), ());
    assert_eq!(
        Release::unlisted_components(set, &game),
        Vec::<String>::new()
    );
    game.insert_parts(id, Unlisted);
    assert_eq!(
        Release::unlisted_components(set, &game),
        [type_name::<Unlisted>()]
    );
}

#[test]
fn each_capability_schedules_its_own_orders_with_no_orders_capability() {
    // The learns are progression's, the cancels and rallies production's, the trades items'; a
    // trade, as a server runs it. No match here declares `orders`.
    let scheduled = |declared: &[Capability]| {
        let budgets = ScriptBudgets::new(ScriptLimits::ROOMY, 1);
        let world = Match::new(
            MatchSpec::new(SegmentSeed::ZERO, Match::TEST_RATE, Role::Server(budgets))
                .set(Release::capability_set(declared).unwrap()),
        );
        let schedules = world.resource::<Schedules>();
        let systems = schedules.get(SimUpdate).unwrap().systems().unwrap();
        let names: Vec<String> = systems
            .map(|(_, system)| system.name().to_string())
            .collect();
        ["learn_ranks", "apply_cancels", "trade_items"]
            .map(|wanted| names.iter().any(|name| name.contains(wanted)))
    };
    let with = |extra: &[Capability]| [&[Stats][..], extra].concat();
    assert_eq!(scheduled(&with(&[])), [false, false, false]);
    assert_eq!(scheduled(&with(&[Progression])), [true, false, false]);
    assert_eq!(scheduled(&with(&[Production])), [false, true, false]);
    assert_eq!(scheduled(&with(&[Items])), [false, false, true]);
    let all = [Progression, Production, Items];
    assert_eq!(scheduled(&with(&all)), [true, true, true]);
}

mod declaration_audit;
mod genre_words;
mod prediction_reads;
