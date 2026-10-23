use campfire_common::Ticks;
use campfire_declarations::{
    BookCx, BookError, BookPlace, ById, ByIdBuilder, NamedScripts, PerId, ScriptParams,
};
use campfire_script::{Ctx, Hook, HookDecl, ScriptId, ScriptRole, Unit};
use campfire_sim::MatchRate;
use campfire_stats::{ModifierAppliers, Way};
use campfire_units::ScriptBook;
use campfire_values::{TimeTooLarge, UnitType};

use crate::ai_data::AiData;
use crate::error::AiError;
use crate::next_turn::{NextTurn, TurnSpec};

/// A unit type's AI: its compiled script, and its period in whole ticks, at least one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ai {
    pub(crate) script: ScriptId,
    pub(crate) period: Ticks,
}

/// `on_think(ctx, unit)`: a unit's AI thinks, each period of its unit type's.
#[derive(Debug)]
pub(crate) struct OnThink;

impl HookDecl for OnThink {
    const NAME: &'static str = "on_think";
    const ROLE: ScriptRole = ScriptRole::Ai;
    const PARAMS: &'static [&'static str] = &["ctx", "unit"];
    type Args = (Ctx, Unit);
}

impl Ai {
    /// The AI of `data`, whose script is `script`, which defines the hooks `scripts` gives, its
    /// period at `rate`, a tick at the least; an error for a period too long to count, or a
    /// script that defines no `on_think`.
    pub fn of(
        data: &AiData,
        script: ScriptId,
        scripts: &ScriptBook,
        rate: MatchRate,
    ) -> Result<Self, AiError> {
        let thinks = scripts
            .defines(Some(script), ScriptRole::Ai)
            .contains(Hook::of::<OnThink>());
        let period = rate.duration(data.think_ms).ok_or(TimeTooLarge)?;
        if !thinks {
            return Err(AiError::NoThink);
        }
        Ok(Self { script, period })
    }
}

impl Ai {
    /// Adds that the AI of each unit type that stands applies, with no action, the modifiers its
    /// script applies; an error for a script the type's package does not hold.
    pub(crate) fn applied_ways(
        cx: &BookCx<'_>,
        appliers: &mut ModifierAppliers,
    ) -> Result<(), BookError> {
        for unit in cx.standing() {
            if let Some(orders) = unit.data.section::<AiData>() {
                appliers.script(cx, unit.package, &orders.ai, &Way::NoAction)?;
            }
        }
        Ok(())
    }

    /// Adds the AI script of each unit type, which serves the AI and reads the mode's params.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn named_scripts(
        cx: &BookCx<'_>,
        scripts: &mut NamedScripts,
    ) -> Result<(), BookError> {
        for unit in cx.types() {
            if let Some(orders) = unit.data.section::<AiData>() {
                scripts.name(unit.package, &orders.ai, ScriptRole::Ai, ScriptParams::Mode);
            }
        }
        Ok(())
    }

    /// The rule of an avatar: its type takes no `orders`, as bots play it.
    pub(crate) fn avatar_rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let ordered = cx
            .types()
            .iter()
            .find(|unit| unit.is_avatar() && unit.data.section::<AiData>().is_some());
        ordered.map_or(Ok(()), |unit| {
            let at = BookPlace::UnitType(unit.place());
            Err(BookError::rule(unit.package, at, AiError::AvatarOrders))
        })
    }

    /// The AI of each unit type that stands and takes orders, its script one of its package's; an
    /// error for a script the package does not hold, or an AI that does not load.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let scripts = cx.book::<ScriptBook>()?;
        let mut book = ByIdBuilder::default();
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
        Ok(book.build())
    }
}

impl PerId<UnitType> for Ai {
    const SCRIPTS_ONLY: bool = true;
}

/// The tick a unit that thinks is due in again, a part of each unit of a type with AI from its
/// spawn. A unit whose call found the think pool spent keeps its tick, so it stays due, and goes
/// first in the next tick, before the units due later. A think writes it anew.
pub type NextThink = NextTurn<Ai>;

impl TurnSpec for Ai {
    const NAME: &'static str = "orders.next_think";

    fn period(&self) -> Ticks {
        self.period
    }
}
