use std::cmp::Ordering;
use std::fmt::Debug;

use crate::error::NamesError;

/// An id that a table of names numbers by its place there.
pub trait NameId: Copy + Debug {
    /// Where a name of the kind is its id's alone: `()` for a kind whose names the match holds
    /// once each.
    type Scope: Copy + Ord + Debug;
    /// The most ids of the kind.
    const LIMIT: usize;

    /// The id at `index`, below `LIMIT`.
    fn at(index: usize) -> Self;

    fn index(self) -> usize;
}

/// What a lookup gives a table of names: a name, in its scope.
pub trait NameKey<S> {
    fn scope(&self) -> S;

    fn text(&self) -> &str;
}

impl NameKey<()> for &str {
    fn scope(&self) {}

    fn text(&self) -> &str {
        self
    }
}

impl<S: Copy> NameKey<S> for (S, &str) {
    fn scope(&self) -> S {
        self.0
    }

    fn text(&self) -> &str {
        self.1
    }
}

/// The names of the ids of one kind: each id's name and scope, by id, every name in one buffer,
/// and the ids sorted by scope, then name, for a binary search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Names<I: NameId> {
    text: String,
    /// Where each id's name ends in `text`.
    ends: Vec<u32>,
    scopes: Vec<I::Scope>,
    sorted: Vec<I>,
}

impl<I: NameId> Names<I> {
    /// Adds the name `key` as the next id, which its scope does not name yet, below `LIMIT`.
    pub fn add(&mut self, key: impl NameKey<I::Scope>) -> I {
        let (scope, text) = (key.scope(), key.text());
        match self.insert(scope, text) {
            Ok(id) => id,
            Err(NamesError::Repeated) => panic!("a scope names an id once: {text}"),
            Err(NamesError::Full) => panic!("the ids of a kind are below its limit: {text}"),
        }
    }

    /// Adds the name `key` as the next id; an error when its scope names it already, or the
    /// table holds `LIMIT` ids.
    pub fn try_add(&mut self, key: impl NameKey<I::Scope>) -> Result<I, NamesError> {
        self.insert(key.scope(), key.text())
    }

    /// Adds `text` of `scope` as the next id, as `try_add` does.
    fn insert(&mut self, scope: I::Scope, text: &str) -> Result<I, NamesError> {
        let Err(at) = self.find(scope, text) else {
            return Err(NamesError::Repeated);
        };
        if self.len() == I::LIMIT {
            return Err(NamesError::Full);
        }
        let id = I::at(self.len());
        self.text.push_str(text);
        let end = u32::try_from(self.text.len()).expect("names fit u32");
        self.ends.push(end);
        self.scopes.push(scope);
        self.sorted.insert(at, id);
        Ok(id)
    }

    /// The table of `keys`, each a name of the first key that names it: a repeat, or a key past
    /// `LIMIT`, has no id. For data a check has yet to refuse them in.
    pub fn first_of<K: NameKey<I::Scope>>(keys: impl IntoIterator<Item = K>) -> Names<I> {
        let mut names = Names::default();
        for key in keys {
            names.try_add(key).ok();
        }
        names
    }

    /// The table of `keys`, each the next id; an error at the first key its scope names already,
    /// or past `LIMIT`.
    pub fn all_of<K: NameKey<I::Scope>>(
        keys: impl IntoIterator<Item = K>,
    ) -> Result<Names<I>, NamesError> {
        let mut names = Names::default();
        for key in keys {
            names.try_add(key)?;
        }
        Ok(names)
    }

    /// The id named `key`.
    pub fn named(&self, key: impl NameKey<I::Scope>) -> Option<I> {
        let at = self.find(key.scope(), key.text()).ok()?;
        Some(self.sorted[at])
    }

    /// The name of `id`; none for an id past the table.
    pub fn name(&self, id: I) -> Option<&str> {
        let index = id.index();
        let end = *self.ends.get(index)? as usize;
        let start = index
            .checked_sub(1)
            .map_or(0, |before| self.ends[before] as usize);
        Some(&self.text[start..end])
    }

    /// The scope of `id`, which the table holds.
    pub fn scope(&self, id: I) -> I::Scope {
        self.scopes[id.index()]
    }

    /// Whether the table holds `id`.
    pub fn has(&self, id: I) -> bool {
        id.index() < self.ends.len()
    }

    pub const fn len(&self) -> usize {
        self.ends.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }

    /// Every id with its name, by id.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (I, &str)> {
        (0..self.len()).map(|at| {
            let id = I::at(at);
            (id, self.name(id).expect("an id below the count"))
        })
    }

    /// Every name, by id.
    pub fn texts(&self) -> impl ExactSizeIterator<Item = &str> {
        self.iter().map(|(_, text)| text)
    }

    /// Where the name `text` of `scope` is in `sorted`, or where it would go.
    fn find(&self, scope: I::Scope, text: &str) -> Result<usize, usize> {
        self.sorted.binary_search_by(|&id| {
            let held = self.name(id).expect("a sorted id is held");
            let order: Ordering = self.scope(id).cmp(&scope);
            order.then_with(|| held.cmp(text))
        })
    }
}

impl<I: NameId> Default for Names<I> {
    fn default() -> Names<I> {
        Names {
            text: String::new(),
            ends: Vec::new(),
            scopes: Vec::new(),
            sorted: Vec::new(),
        }
    }
}

impl<I: NameId, K: NameKey<I::Scope>> FromIterator<K> for Names<I> {
    fn from_iter<T: IntoIterator<Item = K>>(keys: T) -> Names<I> {
        let mut names = Names::default();
        for key in keys {
            names.add(key);
        }
        names
    }
}

impl<I: NameId, K: NameKey<I::Scope>, const N: usize> From<[K; N]> for Names<I> {
    fn from(keys: [K; N]) -> Names<I> {
        keys.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An id of the tests, of at most three names.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Small(u8);

    impl NameId for Small {
        type Scope = ();
        const LIMIT: usize = 3;

        fn at(index: usize) -> Small {
            Small(u8::try_from(index).unwrap())
        }

        fn index(self) -> usize {
            self.0.into()
        }
    }

    /// An id of the tests, its names each in the scope of a package.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Scoped(u8);

    impl NameId for Scoped {
        type Scope = u16;
        const LIMIT: usize = 256;

        fn at(index: usize) -> Scoped {
            Scoped(u8::try_from(index).unwrap())
        }

        fn index(self) -> usize {
            self.0.into()
        }
    }

    #[test]
    fn a_name_finds_its_id_in_its_scope_and_an_id_its_name() {
        // Ids by the order they are added, which a lookup finds whatever their names' order.
        let names = Names::<Small>::from(["b", "", "a"]);
        assert_eq!(names.len(), 3);
        assert_eq!(names.texts().collect::<Vec<_>>(), ["b", "", "a"]);
        assert_eq!(
            ["a", "b", "", "c"].map(|name| names.named(name)),
            [Some(Small(2)), Some(Small(0)), Some(Small(1)), None]
        );
        assert_eq!(
            [0, 1, 2, 3].map(|at| names.name(Small(at))),
            [Some("b"), Some(""), Some("a"), None]
        );
        assert!(names.has(Small(2)) && !names.has(Small(3)));
        assert!(Names::<Small>::default().is_empty());
        // One name in two scopes is two ids; a scope finds only its own.
        let mut scoped = Names::<Scoped>::default();
        let ids = [(1, "x"), (0, "y"), (0, "x")].map(|key| scoped.add(key));
        assert_eq!(ids, [Scoped(0), Scoped(1), Scoped(2)]);
        assert_eq!(
            [(0, "x"), (1, "x"), (1, "y"), (2, "x")].map(|key| scoped.named(key)),
            [Some(Scoped(2)), Some(Scoped(0)), None, None]
        );
        assert_eq!((scoped.scope(Scoped(0)), scoped.scope(Scoped(1))), (1, 0));
    }

    #[test]
    fn a_table_refuses_a_repeat_in_a_scope_and_an_id_past_its_limit() {
        let mut names = Names::<Small>::from(["a", "b"]);
        assert_eq!(names.try_add("a"), Err(NamesError::Repeated));
        assert_eq!(names.try_add("c"), Ok(Small(2)));
        assert_eq!(names.try_add("d"), Err(NamesError::Full));
        // A refused name leaves the table as it was.
        assert_eq!(names.texts().collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(names.try_add("c"), Err(NamesError::Repeated));
        // Data a check has yet to read keeps the first id of each name, and none past the limit.
        let first = Names::<Small>::first_of(["a", "b", "a", "c", "d"]);
        assert_eq!(first.texts().collect::<Vec<_>>(), ["a", "b", "c"]);
        // A table of all its keys or none.
        let all = Names::<Small>::all_of(["a", "b"]).unwrap();
        assert_eq!(all.texts().collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(
            Names::<Small>::all_of(["a", "b", "a"]),
            Err(NamesError::Repeated)
        );
        assert_eq!(
            Names::<Small>::all_of(["a", "b", "c", "d"]),
            Err(NamesError::Full)
        );
    }

    #[test]
    #[should_panic(expected = "a scope names an id once: a")]
    fn an_add_of_a_repeat_fails() {
        drop(Names::<Small>::from(["a", "a"]));
    }
}
