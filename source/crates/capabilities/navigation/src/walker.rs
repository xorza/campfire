use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_math::Num;
use campfire_units::{Body, BodyForm, Layer, MoveStep, Variations};
use campfire_values::UnitType;

use crate::error::NavigationDataError;

/// A kind of unit that walks, as routes see it: the layer it moves on and its body's radius, 0
/// for one with no body. The pathing grid keeps the cells each kind may stand in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Walker {
    pub layer: Layer,
    pub radius: Num,
}

impl Walker {
    /// The walker of a unit with `body`, a circle or none; `None` for a box, which never walks.
    pub const fn of(body: Option<&Body>) -> Option<Self> {
        let radius = match body {
            Some(body) => match body.radius() {
                Some(radius) => radius,
                None => return None,
            },
            None => Num::ZERO,
        };
        Some(Self {
            layer: Body::layer_of(body),
            radius,
        })
    }

    /// The walker of a unit that walks, with `body`: a circle or none, as a box never walks,
    /// which `MoveStep`'s check holds of a snapshot too.
    pub(crate) const fn walking(body: Option<&Body>) -> Self {
        Self::of(body).expect("a unit that walks has a circle for a body, or none")
    }

    /// The walker of a unit type of `form`, a circle or none, as its units' bodies make them;
    /// `None` for a box. A circle's spawn angle turns nothing.
    pub fn of_form(form: Option<BodyForm>) -> Option<Self> {
        Self::of(form.map(|form| form.at(Num::ZERO)).as_ref())
    }
}

impl PerId<UnitType> for Walker {
    const SCRIPTS_ONLY: bool = false;
}

impl Walker {
    /// The kind of walker of each unit type that stands and walks, as its move step says, of the
    /// type's body, the largest of its variations for one that varies; an error for one of a box
    /// body, or of a variation of one.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let bodies = cx.book::<ById<UnitType, BodyForm>>()?;
        let variations = cx.book_if::<ById<UnitType, Variations>>()?;
        let steps = cx.book::<ById<UnitType, MoveStep>>()?;
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            if steps.get(unit.unit_type).is_none() {
                continue;
            }
            let boxed = || {
                let at = BookPlace::UnitType(unit.place());
                BookError::rule(unit.package, at, NavigationDataError::BoxWalks)
            };
            let walker = Self::of_form(bodies.get(unit.unit_type).copied()).ok_or_else(boxed)?;
            let of_type = variations.and_then(|variations| variations.get(unit.unit_type));
            if of_type.is_some_and(|forms| forms.iter().any(|form| form.shape().is_box())) {
                return Err(boxed());
            }
            book.set(unit.unit_type, walker);
        }
        Ok(book.build())
    }
}
