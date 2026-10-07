use std::collections::BTreeSet;
use std::fmt;

use rhai::{AST, ASTNode, Engine, Expr};

/// Registers the getter and setter of one property name on one type, to call its indexer.
pub(crate) type Forward = Box<dyn Fn(&mut Engine, &str)>;

/// The property names the host's scripts use, and how each type that reads and writes its
/// properties through its indexer registers a getter and a setter of a name. Rhai tries a
/// property's getter first; with none, it formats an error message, and only then calls the
/// indexer, each an operation. A forward registers a getter and a setter of each name that call
/// the indexer directly, so the access costs one operation and builds no error.
pub(crate) struct PropertyForwards {
    names: BTreeSet<String>,
    forwards: Vec<Forward>,
}

impl PropertyForwards {
    pub(crate) const fn new() -> PropertyForwards {
        PropertyForwards {
            names: BTreeSet::new(),
            forwards: Vec::new(),
        }
    }

    /// Adds `forward`, and registers it for every name the scripts already use.
    pub(crate) fn add(&mut self, engine: &mut Engine, forward: Forward) {
        for name in &self.names {
            forward(engine, name);
        }
        self.forwards.push(forward);
    }

    /// Registers every forward for each property name `ast` uses that no script used before.
    pub(crate) fn learn(&mut self, engine: &mut Engine, ast: &AST) {
        let (names, forwards) = (&mut self.names, &self.forwards);
        ast.walk(&mut |path: &[ASTNode<'_>]| {
            if let Some(ASTNode::Expr(Expr::Property(property, _))) = path.last() {
                let name = property.2.as_str();
                if !names.contains(name) {
                    for forward in forwards {
                        forward(engine, name);
                    }
                    names.insert(name.to_owned());
                }
            }
            true
        });
    }
}

/// A forward prints only its count.
impl fmt::Debug for PropertyForwards {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PropertyForwards")
            .field("names", &self.names)
            .field("forwards", &self.forwards.len())
            .finish()
    }
}
