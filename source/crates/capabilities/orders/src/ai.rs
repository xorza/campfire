use campfire_common::Ticks;
use campfire_declare::{BookCx, BookError, BookPlace, ByType, PerType};
use campfire_script::{Hook, ScriptId, ScriptRole};
use campfire_sim::TickRate;
use campfire_units::ScriptBook;

use crate::ai_data::AiData;
use crate::error::AiError;

/// A unit type's AI: its compiled script, and its period in whole ticks, at least one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ai {
    pub(crate) script: ScriptId,
    pub(crate) period: Ticks,
}

impl Ai {
    /// The AI of `data`, whose script is `script`, which defines the hooks `scripts` gives, its
    /// period at `rate`, a tick at the least; an error for a period too long to count, or a
    /// script that defines no `on_think`.
    pub fn of(
        data: &AiData,
        script: ScriptId,
        scripts: &ScriptBook,
        rate: TickRate,
    ) -> Result<Self, AiError> {
        let thinks = scripts
            .defines(Some(script), ScriptRole::Ai)
            .contains(Hook::OnThink);
        let period = rate.duration(data.think_ms).ok_or(AiError::TimeTooLarge)?;
        if !thinks {
            return Err(AiError::NoThink);
        }
        Ok(Self { script, period })
    }
}

impl Ai {
    /// The rule of an avatar: its type takes no `orders`, as bots play it.
    pub(crate) fn avatar_rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let ordered = cx
            .types()
            .iter()
            .find(|unit| unit.avatar() && unit.data.section::<AiData>().is_some());
        ordered.map_or(Ok(()), |unit| {
            let at = BookPlace::UnitType(unit.place());
            Err(BookError::rule(unit.package, at, AiError::AvatarOrders))
        })
    }

    /// The AI of each unit type that stands and takes orders, its script one of its package's; an
    /// error for a script the package does not hold, or an AI that does not load.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let scripts = cx.book::<ScriptBook>()?;
        let mut book = ByType::default();
        for unit in cx.standing() {
            let Some(orders) = unit.data.section::<AiData>() else {
                continue;
            };
            let script = cx.script(unit.package, &orders.ai)?;
            let ai = Self::of(orders, script, scripts, cx.rate()).map_err(|error| {
                BookError::rule(unit.package, BookPlace::UnitType(unit.place()), error)
            })?;
            book.set(unit.unit_type, ai);
        }
        Ok(book)
    }
}

impl PerType for Ai {
    const SCRIPTS_ONLY: bool = true;
}
