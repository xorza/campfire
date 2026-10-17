use campfire_actions::{ActionBook, ActionsData};
use campfire_declare::{BookCx, BookError};
use campfire_values::{NameId, Names};

use crate::modifier_book::ModifierBook;
use crate::modifier_param_table::ModifierParamTable;
use crate::param_book::{ParamBook, ParamTables};
use crate::stat_book::StatBook;

/// The actions' params as stats reads them: the book of every action's and modifier's params,
/// which stats builds from the actions' data once their load checked it.
#[derive(Debug)]
pub(crate) struct ActionParams;

impl ActionParams {
    /// The book of every action's and modifier's params, the actions' in the order of their ids,
    /// and where each action holds the params its modifiers read of it.
    pub(crate) fn book(cx: &BookCx<'_>) -> Result<ParamBook, BookError> {
        let mut tables = ParamTables::default();
        if let Some(modifiers) = cx.fact_if::<ModifierParamTable>()? {
            tables.modifiers = modifiers.0.clone();
        }
        let actions = cx.book::<ActionBook>()?;
        let stats = cx.book_if::<StatBook>()?;
        let names = actions.names();
        for (id, name) in names.iter() {
            let data = cx
                .package(names.scope(id))
                .content
                .get::<ActionsData>()
                .and_then(|actions| actions.0.get(name))
                .expect("a loaded action is of its package's data");
            let run = tables.actions.push(&data.params, |stat| {
                stats.and_then(|stats| stats.named(stat)).ok_or(())
            });
            let run = run.expect("the action load checked each stat a param names");
            debug_assert_eq!(run, id.index(), "one run of params per action");
        }
        let none = Names::default();
        let modifiers = cx.book_if::<ModifierBook>()?;
        let appliers = modifiers.map_or(&none, |modifiers| modifiers.appliers());
        Ok(ParamBook::new(tables, appliers))
    }
}
