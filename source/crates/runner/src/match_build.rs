use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_capabilities::{Books, Mode, ModeInputs, ModeSetup, ScriptBook, ScriptBudgets, Units};
use campfire_package::ModePackages;
use campfire_script::ScriptId;
use campfire_sim::{StateRegistry, TickRate};

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
    ) {
        let data = packages.data();
        let ModeInputs { units, books } =
            MatchBuild::books(packages, world, schedule, registry, players);
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
            teams: &packages.manifest().teams,
            players,
            unit_types: units.unit_types,
            avatars: units.avatars,
            loadout: units.loadout,
            walkers: packages.walkers(),
        };
        Mode::install(world, schedule, registry, setup, books);
    }

    /// The first part of `run`: the core and the declared capabilities, every package's scripts,
    /// and the books at the match's rate in place, but the mode's own, which it gives back.
    pub(crate) fn books(
        packages: &ModePackages,
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        players: u32,
    ) -> ModeInputs {
        let manifest = packages.manifest();
        let budgets = ScriptBudgets::new(manifest.script_limits, players);
        manifest
            .capabilities
            .install(world, schedule, registry, Some(budgets));
        for view in packages.packages() {
            for script in &view.package.scripts {
                Units::compile(world, &script.source).expect("the load parsed it");
            }
        }
        let rate = *world.resource::<TickRate>();
        let compiled = world.resource::<ScriptBook>().clone();
        let books = Books::build(&packages.book_input(rate, &compiled)).expect(CHECKED);
        books.install(world)
    }
}
