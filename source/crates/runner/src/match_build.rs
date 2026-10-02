use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_capabilities::{Books, MatchScripts, Mode, ModeSetup, ScriptBook, Units};
use campfire_package::ModePackages;
use campfire_script::ScriptId;
use campfire_sim::{StateRegistry, TickRate};

use crate::error::StartError;

/// What the package load checked, which a match build trusts.
const CHECKED: &str = "the load checked it";

/// A match of a mode, built from its read packages into a world that `SimUpdate::prepare` set
/// up.
#[derive(Debug)]
pub(crate) struct MatchBuild;

impl MatchBuild {
    /// Builds the match of `packages` for `players` players: the core and the declared
    /// capabilities the release has, every package's scripts, the books at the match's rate, and
    /// the mode itself. A declared capability the release does not have yet installs nothing.
    pub(crate) fn run(
        packages: &ModePackages,
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        players: u32,
    ) -> Result<(), StartError> {
        let manifest = packages.manifest();
        let data = packages.data();
        let scripts = MatchScripts {
            limits: manifest.script_limits,
            players,
            damage_kinds: data.combat.damage_kinds.as_slice().into(),
            stats: data.stats.keys().cloned().collect(),
            pools: data.pools.keys().cloned().collect(),
            resources: data.resources.as_slice().into(),
        };
        manifest
            .capabilities
            .install(world, schedule, registry, Some(scripts));
        for view in packages.packages() {
            for script in &view.package.scripts {
                Units::compile(world, &script.source).expect("the load parsed it");
            }
        }
        let rate = *world.resource::<TickRate>();
        let compiled = world.resource::<ScriptBook>().clone();
        let books = Books::build(&packages.book_input(rate, &compiled)).expect(CHECKED);
        let units = books.install(world);
        let mode = &packages
            .packages()
            .next()
            .expect("the mode is a package")
            .package;
        let script = mode.script_index(&data.script).expect(CHECKED);
        let setup = ModeSetup {
            script: ScriptId::nth(script),
            data,
            map: packages.map(),
            teams: &manifest.teams,
            players,
            unit_types: units.unit_types,
            avatars: units.avatars,
            loadout: units.loadout,
            walkers: packages.walkers(),
            max_move_speed: manifest.max_move_speed.get(),
            stat_order: packages.stat_graph().order().expect(CHECKED),
        };
        Mode::install(world, schedule, registry, setup).map_err(StartError::Mode)
    }
}
