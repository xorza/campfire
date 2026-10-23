use std::collections::BTreeMap;

use campfire_common::EnumAll;

use crate::zero_hour::dropped::Losses;
use crate::zero_hour::format::ini::object_ini::{IniObject, ObjectId, ObjectIni};

/// The most variations a unit type holds, as a unit's place among them is a `u8`.
const MOST: usize = 256;

/// Why the import converts an object's `BuildVariations` otherwise than whole, or a map's
/// placement of a variation otherwise than to its parent
/// (09-zero-hour.md, A variation varies the body and the look alone;
/// 09-zero-hour.md, A chassis converts to the parent of its file).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll)]
pub enum VariationDropCause {
    /// A variation names no object: the type holds the others.
    NoObject,
    /// More variations than a type holds: the type holds the first 256.
    TooMany,
    /// A variation differs from the first in more than its body and its look: the type holds the
    /// first alone.
    Unalike,
    /// A variation that several parents name, none of them defined in its own file: a map's
    /// placement of it stays a type of its own.
    Shared,
}

/// The objects of `BuildVariations` as the import converts them, read once from the INI files:
/// each parent one unit type of its variations, which its first fields, as every variation is
/// alike but its body and look; and each variation a map places, its parent's type at its place
/// (04-capabilities/00-overview.md, A unit type may vary its units).
#[derive(Debug, Default)]
pub(crate) struct BuildVariations {
    /// Each parent's variations, by its id, in its line's order.
    parents: BTreeMap<ObjectId, Vec<ObjectId>>,
    /// The parent and the place each variation a map places converts to.
    placed: BTreeMap<ObjectId, PlacedVariation>,
    dropped: Losses<VariationDropCause>,
}

/// What a map's placement of a variation converts to: its parent, and its place among the
/// parent's variations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlacedVariation {
    pub(crate) parent: ObjectId,
    pub(crate) place: u8,
}

impl BuildVariations {
    /// The build variations of `objects`, every file read: of each parent, the objects its line names, of a
    /// variation unlike the first the first alone; of each variation, the parent its own file
    /// defines, else the one parent that names it.
    pub(crate) fn of(objects: &ObjectIni) -> Self {
        let mut variations = Self::default();
        for parent in objects.objects() {
            let names = &parent.build().variations;
            if names.is_empty() {
                continue;
            }
            let mut of_parent: Vec<&IniObject> = Vec::with_capacity(names.len());
            for name in names {
                match objects.named(name) {
                    Some(variation) => of_parent.push(variation),
                    None => (variations.dropped).drop(&parent.name, VariationDropCause::NoObject),
                }
            }
            if of_parent.len() > MOST {
                (variations.dropped).drop(&parent.name, VariationDropCause::TooMany);
                of_parent.truncate(MOST);
            }
            let Some(&first) = of_parent.first() else {
                continue;
            };
            if !of_parent
                .iter()
                .all(|variation| first.varies_alone(variation))
            {
                (variations.dropped).drop(&parent.name, VariationDropCause::Unalike);
                of_parent.truncate(1);
            }
            let ids = of_parent.iter().map(|variation| variation.id).collect();
            variations.parents.insert(parent.id, ids);
        }
        let mut parents_of = BTreeMap::<ObjectId, Vec<PlacedVariation>>::new();
        for (&parent, of_parent) in &variations.parents {
            for (place, &variation) in of_parent.iter().enumerate() {
                let place = u8::try_from(place).expect("a type holds at most 256 variations");
                parents_of
                    .entry(variation)
                    .or_default()
                    .push(PlacedVariation { parent, place });
            }
        }
        for (variation, parents) in parents_of {
            let file = objects[variation].file();
            let own = parents
                .iter()
                .find(|placed| objects[placed.parent].file() == file);
            let placed = match (own, parents.as_slice()) {
                (Some(&own), _) => Some(own),
                (None, &[only]) => Some(only),
                (None, _) => None,
            };
            if let Some(placed) = placed {
                variations.placed.insert(variation, placed);
            } else {
                let name = &objects[variation].name;
                (variations.dropped).drop(name, VariationDropCause::Shared);
            }
        }
        variations
    }

    /// The variations of `parent`, in its line's order; none for an object of none.
    pub(crate) fn of_parent(&self, parent: &IniObject) -> Option<&[ObjectId]> {
        self.parents.get(&parent.id).map(Vec::as_slice)
    }

    /// What a map's placement of `variation` converts to; none for an object that is no
    /// variation, or that is one of several parents none of its file defines.
    pub(crate) fn placed(&self, variation: &IniObject) -> Option<PlacedVariation> {
        self.placed.get(&variation.id).copied()
    }

    /// What it converts otherwise than whole, in the order it met it.
    pub(crate) const fn dropped(&self) -> &Losses<VariationDropCause> {
        &self.dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parent_holds_its_alike_variations_and_a_placed_one_takes_the_parent_of_its_file() {
        // The first file's jeep of two chassis, the second only of another geometry; the second
        // file's toxin jeep of the same chassis, an odd of two that differ in their kinds, and a
        // lost of one chassis and one that no file defines; the third file's two parents of one
        // chassis that the fourth defines.
        let files = [
            "Object Jeep\n BuildVariations = JeepOne JeepTwo\nEnd\n\
             Object JeepOne\nEnd\n\
             ObjectReskin JeepTwo JeepOne\n GeometryMajorRadius = 5.0\nEnd\n",
            "Object Toxin_Jeep\n BuildVariations = JeepOne JeepTwo\nEnd\n\
             Object Odd\n BuildVariations = OddOne OddTwo\nEnd\n\
             Object OddOne\nEnd\n\
             Object OddTwo\n KindOf = STRUCTURE\nEnd\n\
             Object Lost\n BuildVariations = Nowhere LostOne\nEnd\n\
             Object LostOne\nEnd\n",
            "Object FarOne\n BuildVariations = Shared\nEnd\n\
             Object FarTwo\n BuildVariations = Shared\nEnd\n",
            "Object Shared\nEnd\n",
        ];
        let mut ini = ObjectIni::default();
        for file in files {
            ini.read(file.as_bytes()).unwrap();
        }
        let variations = ini.variations();
        let object = |name: &str| ini.named(name).unwrap();
        let of_parent = |name: &str| -> Option<Vec<&str>> {
            let of_parent = ini.variations_of(object(name))?;
            Some(of_parent.map(|variation| variation.name.as_str()).collect())
        };
        // Each parent holds its variations, an unlike one's the first alone, a lost one's those
        // a file defines; an object of none holds none.
        assert_eq!(of_parent("Jeep").unwrap(), ["JeepOne", "JeepTwo"]);
        assert_eq!(of_parent("Toxin_Jeep").unwrap(), ["JeepOne", "JeepTwo"]);
        assert_eq!(of_parent("Odd").unwrap(), ["OddOne"]);
        assert_eq!(of_parent("Lost").unwrap(), ["LostOne"]);
        assert_eq!(of_parent("JeepOne"), None);
        // A parent's type takes its first variation's fields; any other object its own.
        assert_eq!(ini.fielded(object("Jeep")).name, "JeepOne");
        assert_eq!(ini.fielded(object("JeepTwo")).name, "JeepTwo");
        // A placed chassis of the jeep and the toxin jeep takes the jeep, of its own file; the
        // lost chassis its one parent; the unlike chassis, kept by no parent, and the chassis of
        // two parents of another file, none.
        let placed = |name: &str| {
            let placed = variations.placed(object(name))?;
            Some((ini[placed.parent].name.as_str(), placed.place))
        };
        assert_eq!(placed("JeepOne"), Some(("Jeep", 0)));
        assert_eq!(placed("JeepTwo"), Some(("Jeep", 1)));
        assert_eq!(placed("OddOne"), Some(("Odd", 0)));
        assert_eq!(placed("LostOne"), Some(("Lost", 0)));
        assert_eq!(placed("OddTwo"), None);
        assert_eq!(placed("Shared"), None);
        // The losses, the parents' in the order of their names, then the placed chassis'.
        let dropped: Vec<(&str, VariationDropCause)> = variations
            .dropped()
            .into_iter()
            .map(|dropped| (dropped.name.as_str(), dropped.cause))
            .collect();
        assert_eq!(
            dropped,
            [
                ("Lost", VariationDropCause::NoObject),
                ("Odd", VariationDropCause::Unalike),
                ("Shared", VariationDropCause::Shared),
            ]
        );
    }
}
