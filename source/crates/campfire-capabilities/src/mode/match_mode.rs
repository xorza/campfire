use std::ops::Deref;
use std::rc::Rc;

use crate::mode::mode_book::ModeBook;

/// The mode of the match, once it installs, as its systems and checks read it: a non-send
/// resource, as the book holds the script engine's values. A script call reads the same book
/// through its `Ctx`, which holds another handle of it.
#[derive(Debug, Clone)]
pub(crate) struct MatchMode(pub(crate) Rc<ModeBook>);

impl Deref for MatchMode {
    type Target = ModeBook;

    fn deref(&self) -> &ModeBook {
        &self.0
    }
}
