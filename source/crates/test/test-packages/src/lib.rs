//! The workspace's test packages: the test modes, each in its tree on its map, and the tree of a
//! mode in memory, which a test edits before it loads the mode.

mod package_tree;
mod test_mode;

pub use crate::package_tree::{Edit, PackageTree};
pub use crate::test_mode::TestMode;
