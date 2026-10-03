use campfire_math::Num;
use campfire_script::rhai::INT;
use campfire_sim::{Capability, Position};

use crate::navigation::navigation_column::NavigationColumn;
use crate::navigation::navigation_effect::NavigationEffect;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::units::forced_move::DashTo;
use crate::units::unit::Unit;

/// The script API of `navigation`: `unit.path`, and the forced moves `ctx.dash`,
/// `ctx.knock_back` and `ctx.teleport`.
#[derive(Debug)]
pub(crate) struct NavigationApi;

impl NavigationApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let path = MemberSpec::field(
            ApiOwner::Unit,
            "path",
            "the name of the path it walks, `()` with none",
        )
        .capability(Capability::Navigation);
        api.bind(path, |unit: &mut Unit| NavigationColumn::path(unit));
        NavigationApi::register_forced(api);
    }

    /// `ctx.dash`, `ctx.knock_back` and `ctx.teleport`.
    fn register_forced(api: &mut ApiBuilder<'_>) {
        let dash = MemberSpec::call(
            "dash",
            "(unit, to, speed)",
            "moves `unit` on the ground plane at `speed` meters a second to `to`: a point, or a unit it follows until their bodies touch",
        )
        .capability(Capability::Navigation);
        let knock_back = MemberSpec::call(
            "knock_back",
            "(unit, from, distance, ms)",
            "moves `unit` `distance` straight away from `from` over `ms`",
        )
        .capability(Capability::Navigation);
        let teleport = MemberSpec::call(
            "teleport",
            "(unit, pos)",
            "puts `unit` at `pos` at once, or at the nearest place it may stand",
        )
        .capability(Capability::Navigation);
        let point = DashTo::Point;
        let unit = |to: Unit| DashTo::Unit(to.id);
        api.bind(
            dash,
            move |ctx: &mut Ctx, of: Unit, to: Position, speed: Num| {
                NavigationApi::dash(ctx, &of, point(to), speed)
            },
        )
        .bind(
            dash,
            move |ctx: &mut Ctx, of: Unit, to: Position, speed: INT| {
                NavigationApi::dash(ctx, &of, point(to), ApiError::num(speed)?)
            },
        )
        .bind(
            dash,
            move |ctx: &mut Ctx, of: Unit, to: Unit, speed: Num| {
                NavigationApi::dash(ctx, &of, unit(to), speed)
            },
        )
        .bind(
            dash,
            move |ctx: &mut Ctx, of: Unit, to: Unit, speed: INT| {
                NavigationApi::dash(ctx, &of, unit(to), ApiError::num(speed)?)
            },
        )
        .bind(
            knock_back,
            |ctx: &mut Ctx, of: Unit, from: Position, distance: Num, ms: INT| {
                NavigationApi::knock_back(ctx, &of, from, distance, ms)
            },
        )
        .bind(
            knock_back,
            |ctx: &mut Ctx, of: Unit, from: Position, distance: INT, ms: INT| {
                NavigationApi::knock_back(ctx, &of, from, ApiError::num(distance)?, ms)
            },
        )
        .bind(teleport, |ctx: &mut Ctx, of: Unit, to: Position| {
            let teleport = NavigationEffect::teleport(ctx.view(), of.id, to);
            ctx.queue(teleport.map_err(ApiError::fail)?)
        });
    }

    fn dash(ctx: &Ctx, unit: &Unit, to: DashTo, speed: Num) -> Checked<()> {
        let dash = NavigationEffect::dash(ctx.view(), unit.id, to, speed);
        ctx.queue(dash.map_err(ApiError::fail)?)
    }

    fn knock_back(ctx: &Ctx, unit: &Unit, from: Position, distance: Num, ms: INT) -> Checked<()> {
        let knock_back = NavigationEffect::knock_back(ctx.view(), unit.id, from, distance, ms);
        ctx.queue(knock_back.map_err(ApiError::fail)?)
    }
}
