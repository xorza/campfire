use std::fmt;
use std::marker::PhantomData;

use serde::de::{self, DeserializeSeed, EnumAccess, Error, VariantAccess, Visitor};
use serde::{Deserializer, Serializer};

/// An enum of a fixed set whose every member has one name, as data writes it: the derive
/// `NamedEnum` implements it, naming each member by its identifier in snake case or by its
/// `#[name = "…"]`, so a name never leaves its member.
pub trait NamedEnum: Copy + Eq + 'static {
    /// The enum's name, as serde names the type of its unit variants.
    const TYPE: &'static str;
    /// Every member, in order.
    const ALL: &'static [Self];
    /// Each member's name, in the order of `ALL`.
    const NAMES: &'static [&'static str];

    /// The member's place in `ALL`, its discriminant, as the derive gives it.
    fn index(self) -> usize;

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

    /// The member as a unit variant of `TYPE`, by its place and its name, as serde's derive
    /// writes one: a binary format keeps its place, a text format its name.
    fn serialize_member<S: Serializer>(self, serializer: S) -> Result<S::Ok, S::Error> {
        let index = u32::try_from(self.index()).expect("an enum's members fit u32");
        serializer.serialize_unit_variant(Self::TYPE, index, self.name())
    }

    /// The member `deserializer` gives as a unit variant, as `serialize_member` writes it: by its
    /// place, or by the name `name` gives it, so the two never differ. Another name fails, with
    /// every name in the error.
    fn deserialize_member<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_enum(Self::TYPE, Self::NAMES, Members(PhantomData))
    }
}

/// Reads a member of `T` as a unit variant, its identifier by `Members`' own `DeserializeSeed`.
struct Members<T>(PhantomData<T>);

impl<T> fmt::Debug for Members<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Members")
    }
}

impl<'de, T: NamedEnum> Visitor<'de> for Members<T> {
    type Value = T;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "one of {:?}", T::NAMES)
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<T, A::Error> {
        let (member, variant) = data.variant_seed(self)?;
        variant.unit_variant()?;
        Ok(member)
    }

    fn visit_u64<E: Error>(self, index: u64) -> Result<T, E> {
        usize::try_from(index)
            .ok()
            .and_then(T::at)
            .ok_or_else(|| E::invalid_value(de::Unexpected::Unsigned(index), &self))
    }

    fn visit_str<E: Error>(self, name: &str) -> Result<T, E> {
        T::named(name).ok_or_else(|| E::unknown_variant(name, T::NAMES))
    }

    fn visit_bytes<E: Error>(self, name: &[u8]) -> Result<T, E> {
        let name = str::from_utf8(name)
            .map_err(|_| E::invalid_value(de::Unexpected::Bytes(name), &self))?;
        self.visit_str(name)
    }
}

/// A variant's identifier: its place or its name.
impl<'de, T: NamedEnum> DeserializeSeed<'de> for Members<T> {
    type Value = T;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<T, D::Error> {
        deserializer.deserialize_identifier(self)
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::{Binary, Json, Toml};
    use campfire_common_derive::{EnumAll, EnumIdents, NamedEnum};
    use serde::de::value::{Error as ValueError, StrDeserializer};
    use serde::{Deserialize, Serialize};

    use crate::named_enum::NamedEnum;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, NamedEnum)]
    enum Side {
        North,
        South,
        East,
    }

    /// Named by snake case, but the variant that names itself.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, NamedEnum)]
    enum Edge {
        NearSide,
        #[name = "far"]
        FarSide,
        TopEdgeOfIt,
    }

    /// Named each on its own.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, NamedEnum)]
    enum Split {
        #[name = "0_2"]
        ZeroTwo,
        #[name = "1_3"]
        OneThree,
    }

    /// Of discriminants of its own, and with each variant's identifier.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll, EnumIdents)]
    enum Turn {
        Left,
        /// A variant's attributes stay with it.
        Right = 4,
        Back,
    }

    #[test]
    fn all_holds_every_variant_in_the_order_declared() {
        assert_eq!(Side::ALL, [Side::North, Side::South, Side::East]);
        assert_eq!(<Side as NamedEnum>::ALL, Side::ALL);
        assert_eq!(Turn::ALL, [Turn::Left, Turn::Right, Turn::Back]);
        assert_eq!(Turn::ALL.map(|turn| turn as u8), [0, 4, 5]);
        assert_eq!(Turn::IDENTS, ["Left", "Right", "Back"]);
    }

    impl Serialize for Side {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.serialize_member(serializer)
        }
    }

    impl<'de> Deserialize<'de> for Side {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            Self::deserialize_member(deserializer)
        }
    }

    /// The same members, as serde's derive reads and writes them.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum Derived {
        North,
        South,
        East,
    }

    #[test]
    fn a_member_is_named_in_snake_case_or_by_its_own_name() {
        // Snake case puts an underscore before each capital past the first, as serde's
        // `rename_all` does; a member's own name takes its place.
        assert_eq!(Edge::NAMES, ["near_side", "far", "top_edge_of_it"]);
        assert_eq!(Split::NAMES, ["0_2", "1_3"]);
        assert_eq!(Edge::named("far"), Some(Edge::FarSide));
        assert_eq!(Edge::named("far_side"), None);
    }

    #[test]
    fn a_member_reads_by_its_name_at_its_place() {
        assert_eq!(Side::TYPE, "Side");
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
        let read = |text| Side::deserialize_member(StrDeserializer::<ValueError>::new(text));
        assert_eq!(read("south"), Ok(Side::South));
        let error = read("west").unwrap_err().to_string();
        assert_eq!(
            error,
            "unknown variant `west`, expected one of `north`, `south`, `east`"
        );
    }

    /// A member as a TOML data file's field holds it.
    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Field<T> {
        side: T,
    }

    #[test]
    fn a_member_encodes_as_serdes_derive_encodes_its_unit_variant() {
        // Postcard writes its place, JSON and TOML its name, as the derive does; each reads back.
        let pairs = [
            (Side::North, Derived::North),
            (Side::South, Derived::South),
            (Side::East, Derived::East),
        ];
        for (side, derived) in pairs {
            let bytes = Binary::encode(&side);
            assert_eq!(bytes, Binary::encode(&derived));
            assert_eq!(Binary::decode::<Side>(&bytes).ok(), Some(side));
            let text = Json::write(&side);
            assert_eq!(text, Json::write(&derived));
            assert_eq!(Json::parse::<Side>(&text).ok(), Some(side));
            let field = Toml::write(&Field { side });
            assert_eq!(field, Toml::write(&Field { side: derived }));
            assert_eq!(
                Toml::parse::<Field<Side>>(&field).ok(),
                Some(Field { side })
            );
        }
        // A place past the members fails, as a name of none does.
        assert!(Binary::decode::<Side>(&[3]).is_err());
        assert!(Json::parse::<Side>("\"west\"").is_err());
        assert!(Toml::parse::<Field<Side>>("side = \"west\"\n").is_err());
    }
}
