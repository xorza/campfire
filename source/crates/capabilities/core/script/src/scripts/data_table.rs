/// A table of the data files, as the reference titles it: the fields of a data type of the
/// module that reads it, or the keys of a file the modules share, by its title, which tells it from
/// every other (06-script-api.md, A table is its reader's).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DataTable {
    title: &'static str,
}

impl DataTable {
    /// The mode's file, whose keys its modules' sections are.
    pub const MODE: Self = Self::new("`data/mode.toml`");
    /// A unit type, whose keys its modules' sections are.
    pub const UNIT_TYPE: Self = Self::new("A unit type, `[units.<id>]`");

    /// The table the reference titles `title`.
    pub const fn new(title: &'static str) -> Self {
        Self { title }
    }

    /// The table as the reference titles it.
    pub const fn title(self) -> &'static str {
        self.title
    }
}
