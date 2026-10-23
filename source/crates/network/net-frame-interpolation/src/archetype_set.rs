use bevy_ecs::archetype::ArchetypeId;

/// A set of archetypes, one bit for each archetype index, which a system tests for each entity.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ArchetypeSet {
    words: Vec<u64>,
}

impl ArchetypeSet {
    pub const fn new() -> Self {
        Self { words: Vec::new() }
    }

    pub fn insert(&mut self, archetype: ArchetypeId) {
        let index = archetype.index();
        let word = index / 64;
        if word >= self.words.len() {
            self.words.resize(word + 1, 0);
        }
        self.words[word] |= 1 << (index % 64);
    }

    pub fn contains(&self, archetype: ArchetypeId) -> bool {
        let index = archetype.index();
        self.words
            .get(index / 64)
            .is_some_and(|word| word & (1 << (index % 64)) != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An archetype is in the set once inserted, across a word boundary, and no other is.
    #[test]
    fn a_set_holds_exactly_what_it_inserted() {
        let mut set = ArchetypeSet::default();
        for index in [0, 63, 64, 130] {
            set.insert(ArchetypeId::new(index));
        }
        let members: Vec<usize> = (0..200)
            .filter(|&index| set.contains(ArchetypeId::new(index)))
            .collect();
        assert_eq!(members, [0, 63, 64, 130]);
        // Index 130 is bit 2 of word 2, so the set holds three words.
        assert_eq!(set.words.len(), 3);
    }
}
