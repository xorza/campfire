use campfire_script::rhai::ImmutableString;

/// Names of one kind, by id, in the form scripts read them, built once from the book that numbers
/// them so that a read allocates nothing.
#[derive(Debug, Default)]
pub(crate) struct ScriptNames(Vec<ImmutableString>);

impl ScriptNames {
    /// Puts `names`, in order of their ids, in place of those it held.
    pub(crate) fn set<'a>(&mut self, names: impl Iterator<Item = &'a str>) {
        self.0.clear();
        self.0.extend(names.map(ImmutableString::from));
    }

    /// The name at `index`; none past the names.
    pub(crate) fn get(&self, index: usize) -> Option<ImmutableString> {
        self.0.get(index).cloned()
    }

    /// The index of `name`.
    pub(crate) fn position(&self, name: &str) -> Option<usize> {
        self.0.iter().position(|held| held == name)
    }

    pub(crate) const fn len(&self) -> usize {
        self.0.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_found_by_its_id_and_its_id_by_the_name() {
        let mut names = ScriptNames::default();
        names.set(["a", "b"].into_iter());
        assert_eq!((names.get(1), names.get(2)), (Some("b".into()), None));
        assert_eq!((names.position("a"), names.position("c")), (Some(0), None));
        names.set(["c"].into_iter());
        assert_eq!((names.len(), names.position("c")), (1, Some(0)));
    }
}
