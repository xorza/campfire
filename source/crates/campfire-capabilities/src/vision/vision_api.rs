use campfire_math::Num;
use campfire_script::rhai::INT;
use campfire_sim::{Capability, Position};

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_api::status::Status;
use crate::units::tag_property::TagProperty;
use crate::units::unit::Unit;
use crate::vision::vision_column::VisionColumn;
use crate::vision::vision_effect::VisionEffect;

/// The script API and data of `vision` beside the queries the view answers: the sight range, the
/// reveal, and the hidden and detects tag properties.
#[derive(Debug)]
pub(crate) struct VisionApi;

impl VisionApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        VisionApi::register_queries(api);
        let reveal = MemberSpec::call(
            "reveal",
            "(pos, radius, ms)",
            "shows the acting unit's vision group the cells within `radius` of `pos` for `ms`, from this tick's Vision stage; no hidden unit",
        )
        .roles(RoleSet::ACTING)
        .capability(Capability::Vision);
        api.bind_for(
            reveal,
            |ctx: &mut Ctx, pos: Position, radius: Num, ms: INT| {
                VisionApi::reveal(ctx, pos, radius, ms)
            },
        )
        .bind_for(
            reveal,
            |ctx: &mut Ctx, pos: Position, radius: INT, ms: INT| {
                VisionApi::reveal(ctx, pos, ApiError::num(radius)?, ms)
            },
        )
        .tag_property(TagProperty::Hidden, Status::Runs(ApiVersion::FIRST))
        .tag_property(TagProperty::Detects, Status::Runs(ApiVersion::FIRST))
        .data(DataTable::Vision, &["sight_range"], &[]);
    }

    /// Queues a reveal to the acting unit's team of the cells within `radius` of `pos`, for `ms`
    /// rounded up to ticks.
    fn reveal(ctx: &Ctx, pos: Position, radius: Num, ms: INT) -> Checked<()> {
        let view = ctx.view();
        let acting = ctx.acting().and_then(|id| view.row(id));
        let team = acting.ok_or_else(|| ApiError::NoActingUnit.fail())?.team;
        if radius < Num::ZERO {
            return Err(ApiError::NegativeRadius.fail().into());
        }
        let ticks = view.lasting(ms).map_err(ApiError::fail)?;
        ctx.queue(VisionEffect {
            team,
            pos,
            radius,
            ticks,
        })
    }

    /// `unit.can_see`, `ctx.find_visible` and `ctx.nearest_visible`.
    fn register_queries(api: &mut ApiBuilder<'_>) {
        let can_see = MemberSpec::method(
            ApiOwner::Unit,
            "can_see",
            "(unit)",
            "whether its team sees the other unit",
        )
        .capability(Capability::Vision);
        api.bind(can_see, |unit: Unit, other: Unit| {
            VisionColumn::can_see(&unit, &other)
        });
        let visible = MemberSpec::call(
            "find_visible",
            "(of, pos, radius, filter)",
            "as `find`, of the units `of`'s team sees",
        )
        .name(3, NameKind::Filter)
        .capability(Capability::Vision);
        api.bind(
            visible,
            |_: &mut Ctx, of: Unit, pos: Position, radius: Num, filter: &str| {
                VisionColumn::find(&of, pos, radius, filter)
            },
        )
        .bind(
            visible,
            |_: &mut Ctx, of: Unit, pos: Position, radius: INT, filter: &str| {
                VisionColumn::find(&of, pos, ApiError::num(radius)?, filter)
            },
        );
        let nearest = MemberSpec::call(
            "nearest_visible",
            "(of, radius, filter)",
            "the nearest living target, centre to centre, whose body `radius` from the edge of `of`'s reaches, as a weapon's range, that `filter` selects and `of`'s team sees, `()` with none",
        )
        .name(2, NameKind::Filter)
        .capability(Capability::Vision);
        api.bind(
            nearest,
            |_: &mut Ctx, of: Unit, radius: Num, filter: &str| {
                VisionColumn::nearest(&of, radius, filter)
            },
        )
        .bind(
            nearest,
            |_: &mut Ctx, of: Unit, radius: INT, filter: &str| {
                VisionColumn::nearest(&of, ApiError::num(radius)?, filter)
            },
        );
    }
}
