use std::sync::Arc;

use campfire_actions::{
    Amount, EffectError, EffectResolveCx, EffectRule, EffectRuleCx, EffectTo, LaunchLists,
    ListedAt, ListedEffect, ListedEffects, ListedSpec,
};
use campfire_common::DeclaredName;
use campfire_declarations::{BookError, Listed};
use campfire_math::Num;
use campfire_script::{
    ApiBuilder, ApiError, CallError, Checked, Ctx, Frame, MemberDecl, ScriptLasting,
    ScriptPosition, Unit, View,
};
use campfire_sim::Capability;
use campfire_units::DashTo;
use campfire_values::Number;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::navigation_effect::NavigationEffect;

/// `move = { to, speed }` or `move = { from, distance, ms }`: a forced move of the unit the
/// effect applies to, whose other unit is `"source"`, the acting unit, or `"reached"`, the unit
/// the list reached, which its list reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveEffectData {
    /// A dash at `speed` meters a second to `to`, which it follows until their bodies touch.
    Dash { to: EffectTo, speed: Number },
    /// A knock back `distance` away from `from` over `ms`.
    KnockBack {
        from: EffectTo,
        distance: Number,
        ms: Number,
    },
}

/// A listed forced move, its numbers resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MoveEffectSpec {
    Dash {
        to: EffectTo,
        speed: Amount,
    },
    KnockBack {
        from: EffectTo,
        distance: Amount,
        ms: Amount,
    },
}

/// A forced move's other unit, as data names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MoveUnit {
    Source,
    Reached,
}

impl MoveUnit {
    const fn to(self) -> EffectTo {
        match self {
            Self::Source => EffectTo::Source,
            Self::Reached => EffectTo::Reached,
        }
    }
}

/// A dash's `to` and `speed`, or a knock back's `from`, `distance` and `ms`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveFields {
    to: Option<MoveUnit>,
    speed: Option<Number>,
    from: Option<MoveUnit>,
    distance: Option<Number>,
    ms: Option<Number>,
}

impl MoveFields {
    fn data(self) -> Option<MoveEffectData> {
        match self {
            Self {
                to: Some(to),
                speed: Some(speed),
                from: None,
                distance: None,
                ms: None,
            } => Some(MoveEffectData::Dash { to: to.to(), speed }),
            Self {
                to: None,
                speed: None,
                from: Some(from),
                distance: Some(distance),
                ms: Some(ms),
            } => Some(MoveEffectData::KnockBack {
                from: from.to(),
                distance,
                ms,
            }),
            _ => None,
        }
    }
}

/// A dash's fields, or a knock back's, and no other.
impl<'de> Deserialize<'de> for MoveEffectData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        MoveFields::deserialize(deserializer)?
            .data()
            .ok_or_else(|| {
                D::Error::custom(
                    "a dash's `to` and `speed`, or a knock back's `from`, `distance` and `ms`",
                )
            })
    }
}

impl MoveEffectData {
    /// The unit the move is to or away from.
    const fn other(&self) -> EffectTo {
        match *self {
            Self::Dash { to, .. } => to,
            Self::KnockBack { from, .. } => from,
        }
    }
}

impl Listed for MoveEffectData {
    type Set = ListedEffects;

    const KEY: &'static str = "move";

    fn into_listed(self) -> Arc<dyn ListedEffect> {
        Arc::new(self)
    }

    fn api(api: &mut ApiBuilder<'_>) {
        let dash = MemberDecl::call(
            "dash",
            &[&["unit", "to", "speed"]],
            "moves `unit` on the ground plane at `speed` meters a second to `to`: a point, or a unit it follows until their bodies touch",
        );
        let knock_back = MemberDecl::call(
            "knock_back",
            &[&["unit", "from", "distance", "ms"]],
            "moves `unit` `distance` straight away from `from` over `ms`",
        );
        api.call(
            dash,
            |ctx: &mut Ctx, of: Unit, to: ScriptPosition, speed: Num| {
                dash_to(ctx, &of, DashTo::Point(to.get()), speed)
            },
        )
        .call(dash, |ctx: &mut Ctx, of: Unit, to: Unit, speed: Num| {
            dash_to(ctx, &of, DashTo::Unit(to.id), speed)
        })
        .call(
            knock_back,
            |ctx: &mut Ctx, of: Unit, from: ScriptPosition, distance: Num, ms: ScriptLasting| {
                let knock_back =
                    NavigationEffect::knock_back(ctx.view(), of.id, from.get(), distance, ms);
                ctx.queue(knock_back.map_err(ApiError::fail)?)
            },
        );
    }
}

/// Queues a dash of `unit` at `speed` to `to`.
fn dash_to(ctx: &Ctx, unit: &Unit, to: DashTo, speed: Num) -> Checked<()> {
    let dash = NavigationEffect::dash(ctx.view(), &ctx.frame(), unit.id, to, speed);
    ctx.queue(dash.map_err(ApiError::fail)?)
}

impl ListedEffect for MoveEffectData {
    fn needs(&self) -> Option<Capability> {
        Some(Capability::Navigation)
    }

    fn numbers(&self) -> Vec<&Number> {
        match self {
            Self::Dash { speed, .. } => vec![speed],
            Self::KnockBack { distance, ms, .. } => vec![distance, ms],
        }
    }

    fn duration(&self) -> Option<&Number> {
        match self {
            Self::Dash { .. } => None,
            Self::KnockBack { ms, .. } => Some(ms),
        }
    }

    fn modifier(&self) -> Option<&DeclaredName> {
        None
    }

    fn lists(&self) -> Option<&LaunchLists> {
        None
    }

    fn takes_point(&self) -> bool {
        false
    }

    fn rule(&self, cx: &dyn EffectRuleCx) -> Result<(), BookError> {
        if self.other() == EffectTo::Reached && !cx.reaches() {
            return Err(cx.fail(EffectRule::NoUnit));
        }
        Ok(())
    }

    fn resolve(&self, cx: &mut dyn EffectResolveCx) -> Result<Arc<dyn ListedSpec>, EffectError> {
        let spec = match self {
            Self::Dash { to, speed } => MoveEffectSpec::Dash {
                to: *to,
                speed: cx.amount(speed)?,
            },
            Self::KnockBack { from, distance, ms } => MoveEffectSpec::KnockBack {
                from: *from,
                distance: cx.amount(distance)?,
                ms: cx.amount(ms)?,
            },
        };
        Ok(Arc::new(spec))
    }
}

impl ListedSpec for MoveEffectSpec {
    /// Queues the move by the rules of the script's calls, `ctx.dash` and `ctx.knock_back`.
    fn queue(&self, at: ListedAt, frame: &mut Frame, view: &View) -> Result<(), CallError> {
        let id = at.unit(frame);
        let acting = frame.acting();
        let other = |to: EffectTo| match to {
            EffectTo::Reached => Ok(at.reached.unit().expect(
                "the load lets a move name the reached unit only in a list that reaches one",
            )),
            EffectTo::Source => acting.ok_or(ApiError::NoActingUnit),
        };
        let effect = match *self {
            Self::Dash { to, speed } => other(to).and_then(|to| {
                NavigationEffect::dash(view, frame, id, DashTo::Unit(to), speed.number(frame))
            }),
            Self::KnockBack { from, distance, ms } => other(from).and_then(|from| {
                let from = view.row(from).ok_or(ApiError::NoActingUnit)?.position;
                // Whole, as the load checked, so the floor is exact.
                let lasting = ScriptLasting::of(view.rate(), ms.number(frame).floor())?;
                NavigationEffect::knock_back(view, id, from, distance.number(frame), lasting)
            }),
        };
        frame.queue(effect.map_err(CallError::Api)?);
        Ok(())
    }
}
