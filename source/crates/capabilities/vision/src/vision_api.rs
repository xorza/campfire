use campfire_math::Vec3;
use campfire_script::{
    ApiBuilder, ApiError, ApiOwner, ApiVersion, Checked, Ctx, DataTable, Filter, MemberSpec,
    RoleSet, ScriptPosition, ScriptRadius, Status, Unit,
};
use campfire_values::TagProperty;

use crate::vision_column::VisionColumn;
use crate::vision_data::VisionData;
use crate::vision_effect::VisionEffect;

/// The script API and data of `vision` beside the queries the view answers: the sight range, the
/// reveal, and the hidden and detects tag properties.
#[derive(Debug)]
pub struct VisionApi;

impl VisionApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        VisionApi::register_queries(api);
        let reveal = MemberSpec::call(
            "reveal",
            &[&["pos", "radius", "ms"]],
            "shows the acting unit's vision group the cells within `radius` of `pos` for `ms`, from this tick's Vision stage; no hidden unit",
        )
        .roles(RoleSet::ACTING);
        api.call(
            reveal,
            |ctx: &mut Ctx, pos: ScriptPosition, radius: ScriptRadius, ms: i64| {
                VisionApi::reveal(ctx, pos.get(), radius, ms)
            },
        )
        .tag_property(TagProperty::Hidden, Status::Runs(ApiVersion::FIRST))
        .tag_property(TagProperty::Detects, Status::Runs(ApiVersion::FIRST))
        .data::<VisionData>(&["sight_range"], &[])
        .shared_data(DataTable::UnitType, &["vision"], &[]);
    }

    /// Queues a reveal to the acting unit's team of the cells within `radius` of `pos`, for `ms`
    /// rounded up to ticks.
    fn reveal(ctx: &Ctx, pos: Vec3, radius: ScriptRadius, ms: i64) -> Checked<()> {
        let view = ctx.view();
        let acting = ctx.acting().and_then(|id| view.row(id));
        let team = acting.ok_or_else(|| ApiError::NoActingUnit.fail())?.team;
        let radius = radius.get();
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
            &[&["unit"]],
            "whether its team sees the other unit",
        );
        api.method(can_see, |unit: Unit, other: Unit| {
            Ok(VisionColumn::can_see(&unit, &other))
        });
        let visible = MemberSpec::call(
            "find_visible",
            &[&["of", "pos", "radius", "filter"]],
            "as `find`, of the units `of`'s team sees",
        );
        api.call(
            visible,
            |_: &mut Ctx, of: Unit, pos: ScriptPosition, radius: ScriptRadius, filter: Filter| {
                Ok(VisionColumn::find(&of, pos.get(), radius, &filter))
            },
        );
        let nearest = MemberSpec::call(
            "nearest_visible",
            &[&["of", "radius", "filter"]],
            "the nearest living target, center to center, whose body `radius` from the edge of `of`'s reaches, as a weapon's range, that `filter` selects and `of`'s team sees, `()` with none",
        );
        api.call(
            nearest,
            |_: &mut Ctx, of: Unit, radius: ScriptRadius, filter: Filter| {
                Ok(VisionColumn::nearest(&of, radius, &filter))
            },
        );
    }
}
