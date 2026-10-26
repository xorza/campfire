use campfire_declarations::{BookCx, BookError, BookSource, Given};
use campfire_mode::{ModeInputs, ModeTeams};
use campfire_stats::MoveSpeedCap;
use campfire_units::UnitTypes;

use crate::books::Books;
use crate::books::book_cells::{BookCells, CellWalk};
use crate::books::book_input::BookInput;
use crate::books::load_names::LoadNames;
use crate::release::Release;
use crate::release::error::CheckedBooksError;

impl Release {
    /// The books of `input`, which the package load checked, each built by the module of the
    /// match that declares it (02-engine-core.md, Books built by their modules); an error for what
    /// the books cannot hold.
    pub fn books(input: &BookInput<'_>) -> Result<Books, BookError> {
        Self::with_cx(input, |mut cx| {
            cx.build_all()?;
            let inputs = ModeInputs::take(&mut cx)?;
            Ok(Books {
                built: cx.into_books(),
                inputs,
                set: input.set,
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

    /// Checks every rule of `input`'s data that a module of its match declares (02-engine-core.md,
    /// A module checks the rules of its own data), runs `check` with the names its scripts may
    /// name, as the package load checks them, then builds the books as `books` does, from the same
    /// build, so the name tables and facts the rules built serve the books. An error for the first
    /// rule the data breaks, a list of names its book refuses, `check`'s, or what the books
    /// cannot hold.
    pub fn checked_books<E>(
        input: &BookInput<'_>,
        check: impl FnOnce(&LoadNames) -> Result<(), E>,
    ) -> Result<Books, CheckedBooksError<E>> {
        let built = Self::with_cx(input, |mut cx| {
            cx.check_all()?;
            let names = LoadNames::of(&mut cx)?;
            if let Err(error) = check(&names) {
                return Ok(Err(error));
            }
            cx.build_all()?;
            let inputs = ModeInputs::take(&mut cx)?;
            Ok(Ok(Books {
                built: cx.into_books(),
                inputs,
                set: input.set,
            }))
        });
        match built {
            Ok(Ok(books)) => Ok(books),
            Ok(Err(error)) => Err(CheckedBooksError::Check(error)),
            Err(error) => Err(CheckedBooksError::Book(error)),
        }
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
            tick_rate: input.tick_rate,
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
