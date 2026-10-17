use campfire_actions::LoadoutRanks;
use campfire_declare::{BookCx, BookError, BookSource, ByType, Given};
use campfire_mode::{ModeInputs, ModeLoad, ModeMap, ModeTeams, ModeUnits};
use campfire_navigation::{Walker, WalkerKinds};
use campfire_stats::{MoveSpeedCap, StatOrder};

use crate::books::Books;
use crate::books::book_cells::BookCells;
use crate::books::book_input::BookInput;
use crate::release::Release;

/// Whether a build of the books checks the rules of their data first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rules {
    /// The package load's build, of data that comes in.
    Checked,
    /// A build of data that the load checked, or that a test makes.
    Trusted,
}

impl Release {
    /// The books of `input`, which the package load checked, each built by the module of the
    /// match that declares it (02-engine-core.md, Books built by their owners); an error for what
    /// the books cannot hold.
    pub fn books(input: &BookInput<'_>) -> Result<Books, BookError> {
        Self::build_books(input, Rules::Trusted)
    }

    /// The books of `input` as `books` builds them, once every rule of their data that a module
    /// of the match declares holds, as the package load checks it; an error for the first rule
    /// the data breaks.
    pub fn check_books(input: &BookInput<'_>) -> Result<Books, BookError> {
        Self::build_books(input, Rules::Checked)
    }

    fn build_books(input: &BookInput<'_>, rules: Rules) -> Result<Books, BookError> {
        let stat_order = StatOrder(input.stat_order.to_vec());
        let cap = MoveSpeedCap(input.max_move_speed.get());
        let teams = ModeTeams(input.teams.to_vec());
        let mut given = Given::default();
        given.add(input.scripts);
        given.add(&stat_order);
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
            tag_names: input.tag_names,
            mode: &input.data.sections,
            map: input.map.sections(),
            packages: input.packages.clone(),
            given,
        };
        let mut cx = BookCx::new(source);
        Self::declare(input.set, &mut BookCells(&mut cx));
        if rules == Rules::Checked {
            cx.check_all()?;
        }
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
    }
}
