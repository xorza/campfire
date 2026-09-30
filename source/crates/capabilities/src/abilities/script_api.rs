use std::cell::RefCell;
use std::rc::Rc;

use campfire_math::{Num, Vec3};
use campfire_script::Raised;
use campfire_script::rhai::{Array, Dynamic, Engine, EvalAltResult, INT, Map};
use campfire_sim::{Position, StableId};

use crate::abilities::ability_data::Relation;
use crate::abilities::error::ApiError;
use crate::combat::attack_stats::ground_offset;
use crate::combat::damage_kind::DamageKind;
use crate::combat::team::Team;

type Checked<T> = Result<T, Box<EvalAltResult>>;

/// `ctx` in a script: what one call reads, and the effects it queues. Effects apply only after
/// the call returns successfully, in the order queued, so a failed call changes nothing.
#[derive(Debug, Clone)]
pub(crate) struct Ctx(pub(crate) Rc<RefCell<Frame>>);

/// What a call reads and queues.
#[derive(Debug)]
pub(crate) struct Frame {
    /// The living units as the tick's casts began, by stable id.
    pub(crate) units: Rc<[UnitHandle]>,
    pub(crate) params: Map,
    pub(crate) effects: Vec<Effect>,
}

/// A unit as a call sees it: its values when the call began, read-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnitHandle {
    pub(crate) id: StableId,
    pub(crate) pos: Position,
    pub(crate) team: Team,
}

/// An effect a call queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Effect {
    Damage { target: StableId, amount: Num },
}

impl Ctx {
    /// The script API of `on_cast`: `ctx.p`, `ctx.find`, `ctx.damage`, and the `Unit` and `Pos`
    /// handles.
    pub(crate) fn register(engine: &mut Engine) {
        engine
            .register_type_with_name::<Ctx>("Ctx")
            .register_get("p", |ctx: &mut Ctx| ctx.0.borrow().params.clone())
            .register_fn(
                "find",
                |ctx: &mut Ctx, of: UnitHandle, pos: Position, radius: Num, filter: &str| {
                    ctx.find(of, pos, radius, filter)
                },
            )
            .register_fn(
                "find",
                |ctx: &mut Ctx, of: UnitHandle, pos: Position, radius: INT, filter: &str| {
                    ctx.find(of, pos, int(radius)?, filter)
                },
            )
            .register_fn(
                "damage",
                |ctx: &mut Ctx, target: UnitHandle, amount: Num, kind: &str| {
                    ctx.damage(target, amount, kind)
                },
            )
            .register_fn(
                "damage",
                |ctx: &mut Ctx, target: UnitHandle, amount: INT, kind: &str| {
                    ctx.damage(target, int(amount)?, kind)
                },
            );
        engine
            .register_type_with_name::<UnitHandle>("Unit")
            .register_get("pos", |unit: &mut UnitHandle| unit.pos)
            .register_fn("==", |a: UnitHandle, b: UnitHandle| a.id == b.id)
            .register_fn("!=", |a: UnitHandle, b: UnitHandle| a.id != b.id);
        engine.register_type_with_name::<Position>("Pos");
    }

    /// The living units within `radius` of `pos` on the ground plane that `filter` selects
    /// relative to `of`, by stable id.
    fn find(&self, of: UnitHandle, pos: Position, radius: Num, filter: &str) -> Checked<Array> {
        let relation =
            Relation::parse(filter).ok_or_else(|| Raised::error(ApiError::UnknownFilter))?;
        if radius < Num::ZERO {
            return Err(Raised::error(ApiError::NegativeRadius).into());
        }
        let frame = self.0.borrow();
        Ok(frame
            .units
            .iter()
            .filter(|unit| match relation {
                Relation::Enemies => of.team.is_enemy_of(unit.team),
                Relation::Allies => !of.team.is_enemy_of(unit.team),
                Relation::All => true,
            })
            .filter(|unit| Vec3::ZERO.within(ground_offset(pos, unit.pos), radius))
            .map(|&unit| Dynamic::from(unit))
            .collect())
    }

    /// Queues `amount` of `kind` damage to `target`. The kind is checked; until the mode's
    /// `calc_damage` runs, it does not change the amount.
    fn damage(&self, target: UnitHandle, amount: Num, kind: &str) -> Checked<()> {
        DamageKind::parse(kind).ok_or_else(|| Raised::error(ApiError::UnknownDamageKind))?;
        if amount < Num::ZERO {
            return Err(Raised::error(ApiError::NegativeDamage).into());
        }
        self.0.borrow_mut().effects.push(Effect::Damage {
            target: target.id,
            amount,
        });
        Ok(())
    }
}

fn int(value: INT) -> Checked<Num> {
    Num::from_int(value).ok_or_else(|| Raised::error(ApiError::IntegerBeyondNum).into())
}
