use campfire_actions::LoadoutRanks;
use campfire_declare::{BookCx, BookError, BookSource, ByType, Given};
use campfire_mode::{ModeInputs, ModeLoad, ModeMap, ModeTeams, ModeUnits};
use campfire_navigation::{Walker, WalkerKinds};
use campfire_stats::MoveSpeedCap;
use campfire_units::UnitTypes;

use crate::books::Books;
use crate::books::book_cells::{BookCells, CellWalk};
use crate::books::book_input::BookInput;
use crate::books::load_names::LoadNames;
use crate::release::Release;

impl Release {
    /// The books of `input`, which the package load checked, each built by the module of the
    /// match that declares it (02-engine-core.md, Books built by their owners); an error for what
    /// the books cannot hold.
    pub fn books(input: &BookInput<'_>) -> Result<Books, BookError> {
        Self::with_cx(input, |mut cx| {
            cx.build_all()?;
            let units = cx.take_fact::<ModeUnits>()?.unwrap_or_default();
            let map = cx
                .take_fact::<ModeMap>()?
                .expect("the mode's core resolves its map");
            let loadout_ranks = cx.take_fact::<LoadoutRanks>()?.map_or(1, |ranks| ranks.0);
            let built = cx.into_books();
            let walkers = built.get::<ByType<Walker>>();
            let walkers: WalkerKinds = walkers
                .into_iter()
                .flat_map(ByType::values)
                .copied()
                .collect();
            Ok(Books {
                built,
                inputs: ModeInputs {
                    units,
                    load: ModeLoad {
                        map,
                        walkers,
                        loadout_ranks,
                    },
                },
            })
        })
    }

    /// The unit types and the tags of `input`'s match, numbered as its books number them; an
    /// error for a type its scope names already, or past the most types or tags a match holds.
    pub fn unit_types(input: &BookInput<'_>) -> Result<UnitTypes, BookError> {
        Self::with_cx(input, |mut cx| {
            let types = cx.take_fact::<UnitTypes>()?;
            Ok(types.expect("the core declares the unit types"))
        })
    }

    /// Checks every rule of `input`'s data that a module of its match declares, as the package
    /// load does before it builds the books (02-engine-core.md, A module checks the rules of its
    /// own data), and gives the names its scripts may name; an error for the first rule the data
    /// breaks, or for a list of names its book refuses.
    pub fn check_rules(input: &BookInput<'_>) -> Result<LoadNames, BookError> {
        Self::with_cx(input, |mut cx| {
            cx.check_all()?;
            LoadNames::of(&mut cx)
        })
    }

    /// What `run` gives of the build of `input`'s books, every book and fact of its match's
    /// modules declared, none built yet.
    fn with_cx<T>(
        input: &BookInput<'_>,
        run: impl FnOnce(BookCx<'_>) -> Result<T, BookError>,
    ) -> Result<T, BookError> {
        let cap = MoveSpeedCap(input.max_move_speed.get());
        let teams = ModeTeams(input.teams.to_vec());
        let mut given = Given::default();
        given.add(input.scripts);
        given.add(input.script_uses);
        given.add(&cap);
        given.add(&teams);
        given.add(input.data);
        given.add(input.map);
        if let Some(heights) = input.heights {
            given.add(heights);
        }
        let source = BookSource {
            rate: input.rate,
            set: input.set,
            engine_tags: input.engine_tags,
            mode: &input.data.sections,
            map: input.map.sections(),
            packages: input.packages.clone(),
            mode_scripts: input.mode_scripts.clone(),
            given,
        };
        let mut cx = BookCx::new(source);
        let walk = CellWalk::Match;
        Self::declare(input.set, &mut BookCells { cx: &mut cx, walk });
        let walk = CellWalk::Release;
        Self::declare(Self::capabilities(), &mut BookCells { cx: &mut cx, walk });
        run(cx)
    }
}
