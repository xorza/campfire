use campfire_common::DeclaredName;
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_script::UnknownName;
use campfire_stats::ModifierBook;
use campfire_values::{DamageKind, ModifierId, Named, Names, UnitType};

use crate::second_life_data::SecondLifeData;

/// A unit type's second life: the modifier its unit holds in it, and the damage kinds that kill
/// through it (defenses-and-airfields.md, A type may live twice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecondLife {
    pub modifier: ModifierId,
    pub lethal: Box<[DamageKind]>,
}

impl SecondLife {
    /// Whether a damage of `kind` kills through it.
    pub fn is_lethal(&self, kind: DamageKind) -> bool {
        self.lethal.contains(&kind)
    }

    /// The second life of each unit type that stands with a `second_life`, of a modifier of its
    /// package and the mode's damage kinds; an error for a name the match does not have.
    pub(crate) fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let modifiers = cx.book::<ModifierBook>()?;
        let kinds = cx.names_if::<DamageKind>()?;
        let no_kinds = Names::default();
        let kinds = kinds.map_or(&no_kinds, |kinds| &**kinds);
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<SecondLifeData>() else {
                continue;
            };
            let fail = |unknown| {
                BookError::unknown(unit.package, BookPlace::UnitType(unit.place()), unknown)
            };
            let modifier = modifiers
                .names()
                .named((unit.package, data.modifier.as_str()))
                .ok_or_else(|| fail(UnknownName::new(ModifierId::NAMES, &data.modifier)))?;
            let kind = |name: &DeclaredName| {
                kinds
                    .named(name.as_str())
                    .ok_or_else(|| fail(UnknownName::new(DamageKind::NAMES, name)))
            };
            let lethal = data.lethal.iter().map(kind).collect::<Result<_, _>>()?;
            book.set(unit.unit_type, Self { modifier, lethal });
        }
        Ok(book.build())
    }
}

impl PerId<UnitType> for SecondLife {
    const SCRIPTS_ONLY: bool = false;
}
