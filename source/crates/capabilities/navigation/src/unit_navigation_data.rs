use campfire_declarations::{
    BookCx, BookError, ById, ByIdBuilder, PatchError, Patched, PerId, Section, SectionTable,
};
use campfire_math::Num;
use campfire_script::{ApiVersion, Status};
use campfire_script::{DataSection, DataTable};
use campfire_values::UnitType;
use serde::{Deserialize, Deserializer, Serialize};

use crate::gait::Gait;

/// A unit type's `navigation` section: how it moves through a crowd, and its weight against
/// another unit as the two part (04-capabilities/navigation.md, A unit moves by its kind of motion;
/// 04-capabilities/navigation.md, A unit has a weight).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct UnitNavigationData {
    pub(crate) gait: Gait,
    pub(crate) weight: Num,
}

impl UnitNavigationData {
    /// A unit type's with no `navigation` section: one that steps, of weight 1.
    pub const DEFAULT: Self = Self {
        gait: Gait::Step,
        weight: Num::ONE,
    };

    /// A unit of `gait` and `weight`; `None` for a weight that is not positive.
    pub fn new(gait: Gait, weight: Num) -> Option<Self> {
        (weight > Num::ZERO).then_some(Self { gait, weight })
    }

    /// The navigation of each unit type that stands with a `navigation` section.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder may fail, as `Declarations::book` takes it; this one reads no other book"
    )]
    pub(crate) fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            book.set_some(unit.unit_type, unit.data.section::<Self>().copied());
        }
        Ok(book.build())
    }

    /// The navigation of `unit_type` in `book`, `DEFAULT` for a type of none.
    pub(crate) fn of(book: &ById<UnitType, Self>, unit_type: UnitType) -> Self {
        book.get(unit_type).copied().unwrap_or(Self::DEFAULT)
    }

    /// The gait of a unit of `unit_type` in `book`, `DEFAULT`'s for a unit of no type.
    pub(crate) fn gait_of(book: &ById<UnitType, Self>, unit_type: Option<&UnitType>) -> Gait {
        unit_type
            .map_or(Self::DEFAULT, |&unit_type| Self::of(book, unit_type))
            .gait
    }
}

impl PerId<UnitType> for UnitNavigationData {
    const SCRIPTS_ONLY: bool = false;
}

/// A unit type's `navigation` as a tool writes it in part, over the defaults it lies on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitNavigationPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gait: Option<Gait>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<Num>,
}

/// Data is untrusted, so a weight that is not positive fails to read; a field the patch leaves out
/// is `DEFAULT`'s.
impl Patched for UnitNavigationData {
    type Patch = UnitNavigationPatch;

    fn from_patch(patch: UnitNavigationPatch) -> Result<Self, PatchError> {
        let gait = patch.gait.unwrap_or(Self::DEFAULT.gait);
        let weight = patch.weight.unwrap_or(Self::DEFAULT.weight);
        Self::new(gait, weight).ok_or(PatchError::Refused("a unit's weight is positive"))
    }
}

impl<'de> Deserialize<'de> for UnitNavigationData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::read_patch(deserializer)
    }
}

impl DataSection for UnitNavigationData {
    const TABLE: DataTable = DataTable::new("A unit type's `navigation`");
}

impl Section for UnitNavigationData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "navigation";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_steps_at_weight_1_unless_its_section_says_and_its_weight_is_positive() {
        let read =
            |gait, weight| UnitNavigationData::from_patch(UnitNavigationPatch { gait, weight });
        assert_eq!(read(None, None), Ok(UnitNavigationData::DEFAULT));
        assert_eq!(
            UnitNavigationData::DEFAULT,
            UnitNavigationData::new(Gait::Step, Num::ONE).unwrap()
        );
        let tank = UnitNavigationData::new(Gait::Drive, Num::int(40)).unwrap();
        assert_eq!(read(Some(Gait::Drive), Some(Num::int(40))), Ok(tank));
        let refused = Err(PatchError::Refused("a unit's weight is positive"));
        assert_eq!(read(None, Some(Num::ZERO)), refused);
        assert_eq!(read(None, Some(-Num::ONE)), refused);
    }
}
