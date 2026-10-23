use std::slice;

use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_math::Num;
use campfire_values::UnitType;

use crate::body_form::BodyForm;
use crate::collision_data::{CollisionData, VariationsData};
use crate::error::UnitTypeError;

/// A unit type's variations: the body of each, one of which every spawn of the type draws for its
/// unit, so every rule reads the one type
/// (04-capabilities/00-overview.md, A unit type may vary its units).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variations {
    bodies: Box<[BodyForm]>,
}

impl Variations {
    /// The body of the variation at `place`; none past the last.
    pub fn get(&self, place: u8) -> Option<&BodyForm> {
        self.bodies.get(usize::from(place))
    }

    pub fn len(&self) -> usize {
        self.bodies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }

    pub fn iter(&self) -> slice::Iter<'_, BodyForm> {
        self.bodies.iter()
    }

    /// The largest body, which every variation fits, for what reads one body of the type, as a
    /// spawn's room before its draw; of equals, the last.
    pub fn bound(&self) -> BodyForm {
        let bound = |form: &&BodyForm| form.at(Num::ZERO).shape().bound();
        *self
            .iter()
            .max_by_key(bound)
            .expect("a type's variations are some")
    }

    /// The variations of each unit type that stands with `variations`, each body on its layer;
    /// an error for a type of a `collision` beside them, of no variation or more than 256, of a
    /// layer the mode does not declare, or of variations on layers apart, as one type moves on
    /// one layer.
    pub(crate) fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(variations) = unit.data.section::<VariationsData>() else {
                continue;
            };
            let fail = |problem| {
                let at = BookPlace::UnitType(unit.place());
                Err(BookError::rule(unit.package, at, problem))
            };
            if unit.data.section::<CollisionData>().is_some() {
                return fail(UnitTypeError::VariedCollision);
            }
            if variations.0.is_empty() || variations.0.len() > usize::from(u8::MAX) + 1 {
                return fail(UnitTypeError::VariationCount(variations.0.len()));
            }
            let bodies: Self = (variations.0.iter())
                .map(|variation| variation.collision.form(cx, unit))
                .collect::<Result<_, _>>()?;
            if bodies
                .iter()
                .any(|form| form.layer() != bodies.bodies[0].layer())
            {
                return fail(UnitTypeError::VariationLayers);
            }
            book.set(unit.unit_type, bodies);
        }
        Ok(book.build())
    }
}

impl FromIterator<BodyForm> for Variations {
    fn from_iter<I: IntoIterator<Item = BodyForm>>(bodies: I) -> Self {
        Self {
            bodies: bodies.into_iter().collect(),
        }
    }
}

impl<const N: usize> From<[BodyForm; N]> for Variations {
    fn from(bodies: [BodyForm; N]) -> Self {
        bodies.into_iter().collect()
    }
}

impl<'a> IntoIterator for &'a Variations {
    type Item = &'a BodyForm;
    type IntoIter = slice::Iter<'a, BodyForm>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl PerId<UnitType> for Variations {
    const SCRIPTS_ONLY: bool = false;
}
