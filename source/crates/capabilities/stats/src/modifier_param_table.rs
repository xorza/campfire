use campfire_declare::{BookCx, BookError, BookPlace, Fact, Section};

use crate::modifiers_data::ModifiersData;
use crate::param_table::ParamTable;
use crate::stat_book::StatBook;

/// Every modifier's params, one run per modifier, in the order the modifier book numbers them:
/// what the param book takes of the modifiers.
#[derive(Debug, Clone, Default)]
pub struct ModifierParamTable(pub ParamTable);

impl Fact for ModifierParamTable {}

impl ModifierParamTable {
    /// The params of every package's modifiers, package by package, each by name; an error for a
    /// stat the mode does not declare.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let stats = cx.book::<StatBook>()?;
        let mut table = ParamTable::default();
        for (index, package) in cx.packages() {
            let modifiers = package.content.get::<ModifiersData>().into_iter();
            for (name, data) in modifiers.flat_map(|modifiers| &modifiers.0) {
                table.push(&data.params, |stat| {
                    let at = BookPlace::Entry {
                        table: ModifiersData::KEY,
                        name: name.clone(),
                    };
                    stats.named_in(index, at, stat)
                })?;
            }
        }
        Ok(Self(table))
    }
}
