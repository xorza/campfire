use serde::Deserializer;
use serde::de::{Deserialize, Error};

/// An enum of a fixed set whose every member has one name, as data writes it: the members are
/// `enum_all!`'s `ALL`, and the names one table in the same order, so neither can lack one.
pub trait NamedEnum: Copy + Eq + 'static {
    /// Every member, in order.
    const ALL: &'static [Self];
    /// Each member's name, in the order of `ALL`.
    const NAMES: &'static [&'static str];

    /// The member's place in `ALL`.
    fn index(self) -> usize {
        const {
            assert!(
                Self::ALL.len() == Self::NAMES.len(),
                "a name for each member"
            );
        };
        let at = Self::ALL.iter().position(|&member| member == self);
        at.expect("`ALL` holds every member")
    }

    /// The member at `index` in `ALL`.
    fn at(index: usize) -> Option<Self> {
        const {
            assert!(
                Self::ALL.len() == Self::NAMES.len(),
                "a name for each member"
            );
        };
        Self::ALL.get(index).copied()
    }

    /// The member's name.
    fn name(self) -> &'static str {
        Self::NAMES[self.index()]
    }

    /// The member named `name`.
    fn named(name: &str) -> Option<Self> {
        let at = Self::NAMES.iter().position(|&named| named == name)?;
        Self::at(at)
    }

    /// The member the text of `deserializer` names: data reads a member by the name `name`
    /// gives it, so the two never differ. Another text fails, with every name in the error.
    fn deserialize_named<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::named(&text).ok_or_else(|| D::Error::unknown_variant(&text, Self::NAMES))
    }
}

#[cfg(test)]
mod tests {
    use serde::de::value::{Error as ValueError, StrDeserializer};

    use crate::named_enum::NamedEnum;

    crate::enum_all! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Side {
            North,
            South,
            East,
        }
        const ALL;
    }

    impl NamedEnum for Side {
        const ALL: &'static [Self] = &Self::ALL;
        const NAMES: &'static [&'static str] = &["north", "south", "east"];
    }

    #[test]
    fn a_member_reads_by_its_name_at_its_place() {
        assert_eq!(Side::ALL.map(Side::index), [0, 1, 2]);
        assert_eq!(Side::ALL.map(Side::name), ["north", "south", "east"]);
        assert_eq!(
            ["north", "south", "east", "North", ""].map(Side::named),
            [
                Some(Side::North),
                Some(Side::South),
                Some(Side::East),
                None,
                None
            ]
        );
        assert_eq!(
            [0, 2, 3].map(Side::at),
            [Some(Side::North), Some(Side::East), None]
        );
        let read = |text| Side::deserialize_named(StrDeserializer::<ValueError>::new(text));
        assert_eq!(read("south"), Ok(Side::South));
        let error = read("west").unwrap_err().to_string();
        assert_eq!(
            error,
            "unknown variant `west`, expected one of `north`, `south`, `east`"
        );
    }
}
