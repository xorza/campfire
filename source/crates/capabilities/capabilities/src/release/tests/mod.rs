use std::any::type_name;
use std::fmt::Debug;
use std::num::NonZeroU64;
use std::path::Path;

use Capability::{
    Abilities, Combat, Items, Mode, Navigation, Orders, Production, Progression, Projectiles,
    Stats, Vision,
};
use bevy_ecs::component::Component;
use bevy_ecs::schedule::Schedules;
use bevy_ecs::world::Mut;
use campfire_actions::ActionBook;
use campfire_codec::Toml;
use campfire_common::SegmentSeed;
use campfire_declarations::{ByType, EntityParts, Module, Role};
use campfire_math::Vec3;
use campfire_orders::Ai;
use campfire_players::{Player, PlayerResources};
use campfire_script::{PlannedCapability, ScriptLimits, ScriptPools, View};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{SimUpdate, SimWorld, StateRegistry};
use campfire_store::{DirEntries, EntryKind, InputFile};
use campfire_units::UnitTypeCore;
use campfire_units::internals::Spawn;
use serde::Deserialize;

use super::*;
use crate::release::r#match::Match;
use crate::release::match_kind::MatchKind;

#[test]
fn a_set_holds_each_capability_once_with_what_it_builds_on() {
    let set = Release::capability_set(&[Orders, Navigation, Stats, Combat, Vision]).unwrap();
    assert_eq!(
        set.iter().collect::<Vec<_>>(),
        [Combat, Stats, Orders, Navigation, Vision]
    );
    assert!(set.contains(Orders) && !set.contains(Abilities));
    assert_eq!(Release::capability_set(&[]).unwrap().iter().count(), 0);
    // Every capability a manifest may declare is a module's of the release.
    let rows = Release::rows();
    let unbuilt = Capability::ALL
        .into_iter()
        .filter(|&capability| capability != Mode);
    let unbuilt: Vec<Capability> = unbuilt
        .filter(|&capability| rows.needs(capability).is_none())
        .collect();
    assert_eq!(unbuilt, []);
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
fn installed(declared: &[Capability], pools: Option<ScriptPools>) -> Match {
    let role = pools.map_or(Role::Rules, Role::Server);
    Match::builder(declared).role(role).build()
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
    let scripts = ScriptPools { limits, players: 1 };
    let mut scripted = installed(&all, Some(scripts));
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
        let player = EntityParts::<Player>::spawn(&mut scripted, ());
        assert_eq!(
            scripted.get::<PlayerResources>(player),
            Some(&PlayerResources::new(0))
        );
    }
    let combat_only = installed(&[Stats, Combat], Some(scripts));
    assert!(combat_only.contains_resource::<ActionBook>());
}

#[test]
fn every_pair_of_capabilities_with_only_what_they_need_orders_every_pair_of_their_systems() {
    // A step that a module orders against reaches it only through the modules installed, and
    // the strict schedule refuses a pair of systems no step orders as the first tick builds it:
    // so the match of each pair of capabilities, each with only what it needs, builds, for a
    // server that runs scripts, one that runs none, and a predicting client. Production and
    // navigation alone are such a pair: production's builds and gathers, which navigation
    // brings, order against the actions' steps with no stats to order through.
    let rows = Release::rows();
    let limits = ScriptLimits {
        per_call: NonZeroU64::MIN,
        player: 1,
        think: 1,
        mode: 1,
    };
    let scripts = ScriptPools { limits, players: 1 };
    let built: Vec<Capability> = Capability::ALL
        .into_iter()
        .filter(|&capability| capability != Mode)
        .collect();
    let mut matches = 0;
    for (at, &first) in built.iter().enumerate() {
        for &second in &built[at..] {
            let mut declared = vec![first, second];
            declared.dedup();
            let mut next = 0;
            while let Some(&capability) = declared.get(next) {
                let needs = rows
                    .needs(capability)
                    .expect("a built capability has its row");
                for &need in needs {
                    if !declared.contains(&need) {
                        declared.push(need);
                    }
                }
                next += 1;
            }
            for mut game in [
                installed(&declared, Some(scripts)),
                installed(&declared, None),
                Match::builder(&declared).role(Role::Client).build(),
            ] {
                game.step();
                matches += 1;
            }
        }
    }
    // 12 capabilities a manifest declares: 12 alone and 12 * 11 / 2 = 66 pairs, three matches
    // each.
    assert_eq!(built.len(), 12);
    assert_eq!(matches, (12 + 66) * 3);
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

/// The capability table (04-capabilities/00-overview.md, Capabilities), beside this crate.
/// The most bytes a test reads of a design document: far past any the design holds.
const DOC_LEN: usize = 1 << 20;

const OVERVIEW: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../docs/design/04-capabilities/00-overview.md"
);

#[test]
fn the_design_names_each_capability_and_marks_built_exactly_those_the_release_installs() {
    // Each row of the table names its capabilities in its first column and its status in the
    // second. Every capability but `mode`, which every match has, is in it once, and built; a
    // planned one is the design's alone, as the script API plans names with it.
    let overview = InputFile::read_text(Path::new(OVERVIEW), DOC_LEN).unwrap();
    let table = overview
        .lines()
        .skip_while(|line| !line.starts_with("| Capability | Status |"))
        .skip(2)
        .take_while(|line| line.starts_with('|'));
    let (mut built, mut planned) = (Vec::new(), Vec::new());
    for row in table {
        let cells: Vec<&str> = row.split('|').map(str::trim).collect();
        let row_names = cells[1].split(", ").map(|name| name.trim_matches('`'));
        match cells[2] {
            "built" => built.extend(row_names),
            "planned" => planned.extend(row_names),
            status => panic!("{status:?} is no status, in {row}"),
        }
    }
    let mut every: Vec<&str> = Capability::ALL
        .into_iter()
        .filter(|&capability| capability != Mode)
        .map(Capability::name)
        .collect();
    every.sort_unstable();
    built.sort_unstable();
    assert_eq!(built, every);
    planned.sort_unstable();
    let design = PlannedCapability::ALL
        .into_iter()
        .map(PlannedCapability::name);
    let mut design: Vec<&str> = design.collect();
    design.sort_unstable();
    assert_eq!(planned, design);
    let mut installed: Vec<&str> = Release::rows()
        .0
        .iter()
        .map(|row| row.capability.name())
        .collect();
    installed.sort_unstable();
    assert_eq!(built, installed);
}

/// The capability crates of this crate's group, each by its directory, as `core/units`, with the
/// capability crates it depends on, by name, as its manifest lists them: the graph the layers
/// come from (02-engine-core.md, Layers). This crate sits above them all.
#[derive(Debug)]
struct CapabilityCrates(Vec<CapabilityCrate>);

/// The most bytes a crate's manifest holds: far past any of the group's.
const MANIFEST_LEN: usize = 1 << 16;

/// What a crate's manifest gives the layers: the crates it depends on, by name.
#[derive(Debug, Deserialize)]
struct Dependencies {
    #[serde(default)]
    dependencies: BTreeMap<String, TomlValue>,
}

/// A capability crate and the capability crates it depends on.
#[derive(Debug)]
struct CapabilityCrate {
    crate_dir: String,
    depends: Vec<String>,
}

impl CapabilityCrates {
    /// This crate's directory in its group.
    const OWN: &str = "capabilities";

    /// The group's directory, which holds this crate's beside the capability crates.
    fn group() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the crate is in its group")
    }

    /// Each directory of the group, or of its `core`, that holds a manifest, but this crate's.
    fn read() -> Self {
        let root = Self::group();
        let mut crate_dirs = Vec::new();
        for parent in ["", "core/"] {
            for entry in DirEntries::read(&root.join(parent)).unwrap() {
                let crate_dir = format!("{parent}{}", entry.name.to_str().unwrap());
                if crate_dir == Self::OWN {
                    continue;
                }
                let held = DirEntries::read(&root.join(&crate_dir)).ok();
                let manifest = held
                    .is_some_and(|entries| entries.iter().any(|inner| inner.name == "Cargo.toml"));
                if entry.kind == EntryKind::Dir && manifest {
                    crate_dirs.push(crate_dir);
                }
            }
        }
        crate_dirs.sort_unstable();
        let name = |crate_dir: &str| crate_dir.rsplit('/').next().unwrap().to_owned();
        let names: Vec<String> = crate_dirs.iter().map(|crate_dir| name(crate_dir)).collect();
        let crates = crate_dirs
            .into_iter()
            .map(|crate_dir| {
                let path = root.join(&crate_dir).join("Cargo.toml");
                let manifest: Dependencies =
                    Toml::parse(&InputFile::read_text(&path, MANIFEST_LEN).unwrap()).unwrap();
                let depends = (manifest.dependencies.keys())
                    .filter_map(|dependency| dependency.strip_prefix("campfire-"))
                    .filter(|dependency| names.iter().any(|name| name == dependency))
                    .map(str::to_owned)
                    .collect();
                CapabilityCrate { crate_dir, depends }
            })
            .collect();
        Self(crates)
    }

    /// The crate whose directory ends in `name`.
    fn named(&self, name: &str) -> Option<&CapabilityCrate> {
        self.0
            .iter()
            .find(|krate| krate.crate_dir.rsplit('/').next() == Some(name))
    }

    /// The layer of the crate `name`: 0 for one that depends on no capability crate, else one
    /// above its highest dependency's, so a crate depends only on crates of lower layers.
    fn layer(&self, name: &str) -> Option<u8> {
        let krate = self.named(name)?;
        let below = krate.depends.iter().map(|dependency| {
            self.layer(dependency)
                .expect("a capability crate's dependency is one")
        });
        Some(below.map(|layer| layer + 1).max().unwrap_or(0))
    }
}

#[test]
fn the_layers_follow_the_capability_crates_manifests() {
    // The reader sees each crate and its dependencies: the lowest crates depend on none, and
    // the mode builds on stats, whose layer is below its own.
    let crates = CapabilityCrates::read();
    assert!(crates.0.len() > 20, "{crates:?}");
    assert_eq!(crates.layer("visibility"), Some(0));
    assert!(
        crates
            .named("mode")
            .unwrap()
            .depends
            .contains(&"stats".to_owned())
    );
    assert!(crates.layer("stats") < crates.layer("mode"));
    assert_eq!(crates.layer("release"), None);
}

/// Each module of the release's list, by the directory of its crate, with what it needs and what it
/// uses: a capability's by its name, the mode's as its scripted part states them, which the
/// mode's core, a part of its crate, builds on none of; and a core module's by its crate, which
/// needs and uses no capability, but the release's own.
#[derive(Debug, Default)]
struct Edges(Vec<(&'static str, &'static [Capability], &'static [Capability])>);

impl ModuleVisitor for Edges {
    fn module<M: Module>(&mut self) {
        match M::CAPABILITY {
            Some(Mode) => {}
            Some(capability) => self.0.push((capability.name(), M::NEEDS, M::USES)),
            None => {
                let path = type_name::<M>();
                let krate = path
                    .split("::")
                    .next()
                    .expect("a type's path has its crate");
                // The release's own module, which builds the match, is of this crate, above every
                // capability.
                if krate == "campfire_capabilities" {
                    return;
                }
                let crate_dir = krate
                    .strip_prefix("campfire_")
                    .expect("a core module is of a campfire crate");
                self.0.push((crate_dir, M::NEEDS, M::USES));
            }
        }
    }

    fn scripted<M: Module>(&mut self) {
        let capability = M::CAPABILITY.expect("the scripted part is the mode's");
        self.0.push((capability.name(), M::NEEDS, M::USES));
    }
}

#[test]
fn a_capabilitys_crate_depends_on_exactly_what_it_needs_or_uses() {
    let crates = CapabilityCrates::read();
    let mut rows = Edges::default();
    Release::modules(&mut rows);
    let mut found = Vec::new();
    // The core modules are checked too: the actions' among them depends on no capability.
    assert!(rows.0.iter().any(|&(name, ..)| name == "actions"));
    for (name, needs, uses) in rows.0 {
        let krate = crates
            .named(name)
            .expect("each module of the release is a crate's");
        let mut depends: Vec<String> = (krate.depends.iter())
            .filter(|dependency| {
                Capability::ALL
                    .iter()
                    .any(|other| other.name() == dependency.as_str())
            })
            .cloned()
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

#[test]
fn each_capability_installs_after_the_lower_layers_it_builds_on() {
    // The table's install order, its needs and the layers are three facts of one order: a
    // capability builds only on capabilities of lower layers, installed before it.
    let crates = CapabilityCrates::read();
    let mut installed = Vec::new();
    for row in Release::rows().0 {
        let own =
            (crates.layer(row.capability.name())).expect("an installed capability is a crate");
        for &need in row.needs {
            assert!(
                installed.contains(&need),
                "{need:?} before {:?}",
                row.capability
            );
            assert!(
                crates.layer(need.name()) < Some(own),
                "{need:?} below {:?}",
                row.capability
            );
        }
        installed.push(row.capability);
    }
}

#[test]
fn a_unit_holds_only_parts_the_declarations_list() {
    // The mode's module, which the capability of the same name hides here.
    // A match of every capability the release runs: a unit holds only parts the declarations
    // list, as state or derived; one more is named.
    /// A part of a unit that no declaration lists.
    #[derive(Component, Debug)]
    struct Unlisted;
    let set = Release::capabilities();
    let declared: Vec<Capability> = set.iter().collect();
    let mut game = Match::builder(&declared).role(Role::Rules).build();
    let unit_type = Units::load_next_type(&mut game, &UnitTypeCore::default());
    let id = Spawn::of(unit_type, Vec3::ZERO).place(&mut game);
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
        let budgets = ScriptPools {
            limits: ScriptLimits::ROOMY,
            players: 1,
        };
        let setup = MatchSetup::new(SegmentSeed::ZERO, Match::TEST_RATE, Role::Server(budgets))
            .set(Release::capability_set(declared).unwrap());
        let world = Match::new(
            setup,
            Release::empty_books(Match::TEST_RATE),
            MatchKind::Rules,
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

mod books;
mod declaration_audit;
mod feature_installs;
mod patches;
mod prediction_reads;
