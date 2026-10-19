use derive_more::Display;

/// The kind of a name that scripts and data give, as the load checks it and its errors say it:
/// the type whose values the names are gives it, through `Named`, with its text, its article and
/// its scope (08-script-api.md, A name kind is its type's).
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[display("{text}")]
pub struct NameKind {
    text: &'static str,
    article: Article,
    scope: NameScope,
}

/// The article a name kind's text takes: "a unit type", "an armor".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Article {
    A,
    An,
}

/// Where a name of a kind means one thing: in the whole match, or only in a package, or a
/// script, so its text alone may name another package's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NameScope {
    Match,
    Package,
}

/// A type whose values scripts and data name by text: the kind of name each is.
pub trait Named {
    const NAMES: NameKind;
}

impl NameKind {
    pub const fn new(text: &'static str, article: Article, scope: NameScope) -> Self {
        Self {
            text,
            article,
            scope,
        }
    }

    /// The article its text takes in the reference's text.
    pub const fn article(self) -> &'static str {
        match self.article {
            Article::A => "a",
            Article::An => "an",
        }
    }

    /// Whether a name of this kind means one thing only in a package, or a script: a name of it
    /// as text, without its scope, may name another package's.
    pub const fn scoped(self) -> bool {
        matches!(self.scope, NameScope::Package)
    }
}
