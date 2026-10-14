use campfire_math::Num;
use campfire_script::{ApiBuilder, ApiError, ApiOwner, Checked, Ctx, DataTable, MemberSpec, Unit};
use campfire_sim::Position;
use campfire_units::DashTo;

use crate::mode_navigation_data::ModeNavigationData;
use crate::navigation_column::NavigationColumn;
use crate::navigation_effect::NavigationEffect;

/// The script API of `navigation`: `unit.path`, and the forced moves `ctx.dash`,
/// `ctx.knock_back` and `ctx.teleport`.
#[derive(Debug)]
pub struct NavigationApi;

impl NavigationApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let path = MemberSpec::field(
            ApiOwner::Unit,
            "path",
            "the name of the path it walks, `()` with none",
        );
        api.bind(path, |unit: &mut Unit| NavigationColumn::path(unit));
        NavigationApi::register_forced(api);
    }

    /// `ctx.dash`, `ctx.knock_back` and `ctx.teleport`.
    fn register_forced(api: &mut ApiBuilder<'_>) {
        let dash = MemberSpec::call(
            "dash",
            &[&["unit", "to", "speed"]],
            "moves `unit` on the ground plane at `speed` meters a second to `to`: a point, or a unit it follows until their bodies touch",
        );
        let knock_back = MemberSpec::call(
            "knock_back",
            &[&["unit", "from", "distance", "ms"]],
            "moves `unit` `distance` straight away from `from` over `ms`",
        );
        let teleport = MemberSpec::call(
            "teleport",
            &[&["unit", "pos"]],
            "puts `unit` at `pos` at once, or at the nearest place it may stand",
        );
        let point = DashTo::Point;
        let unit = |to: Unit| DashTo::Unit(to.id);
        api.call(
            dash,
            move |ctx: &mut Ctx, of: Unit, to: Position, speed: Num| {
                NavigationApi::dash(ctx, &of, point(to), speed)
            },
        )
        .call(
            dash,
            move |ctx: &mut Ctx, of: Unit, to: Position, speed: i64| {
                NavigationApi::dash(ctx, &of, point(to), ApiError::num(speed)?)
            },
        )
        .call(
            dash,
            move |ctx: &mut Ctx, of: Unit, to: Unit, speed: Num| {
                NavigationApi::dash(ctx, &of, unit(to), speed)
            },
        )
        .call(
            dash,
            move |ctx: &mut Ctx, of: Unit, to: Unit, speed: i64| {
                NavigationApi::dash(ctx, &of, unit(to), ApiError::num(speed)?)
            },
        )
        .call(
            knock_back,
            |ctx: &mut Ctx, of: Unit, from: Position, distance: Num, ms: i64| {
                NavigationApi::knock_back(ctx, &of, from, distance, ms)
            },
        )
        .call(
            knock_back,
            |ctx: &mut Ctx, of: Unit, from: Position, distance: i64, ms: i64| {
                NavigationApi::knock_back(ctx, &of, from, ApiError::num(distance)?, ms)
            },
        )
        .call(teleport, |ctx: &mut Ctx, of: Unit, to: Position| {
            let teleport = NavigationEffect::teleport(ctx.view(), of.id, to);
            ctx.queue(teleport.map_err(ApiError::fail)?)
        })
        .shared_data(DataTable::Mode, &["navigation"], &[])
        .data::<ModeNavigationData>(&["layers", "spawn_tags"], &[]);
    }

    fn dash(ctx: &Ctx, unit: &Unit, to: DashTo, speed: Num) -> Checked<()> {
        let dash = NavigationEffect::dash(ctx.view(), &ctx.frame(), unit.id, to, speed);
        ctx.queue(dash.map_err(ApiError::fail)?)
    }

    fn knock_back(ctx: &Ctx, unit: &Unit, from: Position, distance: Num, ms: i64) -> Checked<()> {
        let knock_back = NavigationEffect::knock_back(ctx.view(), unit.id, from, distance, ms);
        ctx.queue(knock_back.map_err(ApiError::fail)?)
    }
}
