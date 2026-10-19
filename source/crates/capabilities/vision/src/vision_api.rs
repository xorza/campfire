use campfire_math::Vec3;
use campfire_script::{
    ApiBuilder, ApiError, ApiOwner, ApiVersion, Checked, Ctx, Filter, MemberSpec, ScriptLasting,
    ScriptPosition, ScriptRadius, Status, Unit,
};
use campfire_values::{TagProperty, Team};

use crate::vision_column::VisionColumn;
use crate::vision_data::VisionData;
use crate::vision_effect::VisionEffect;

/// The script API and data of `vision` beside the queries the view answers: the sight range, the
/// reveal, and the hidden and detects tag properties.
#[derive(Debug)]
pub struct VisionApi;

impl VisionApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        Self::register_queries(api);
        let reveal = MemberSpec::call(
            "reveal",
            &[&["pos", "radius", "ms"], &["pos", "radius", "ms", "team"]],
            "shows `team`'s vision group, else the acting unit's, the cells within `radius` of `pos` for `ms`, from this tick's Vision stage; no hidden unit",
        );
        api.call(
            reveal,
            |ctx: &mut Ctx, pos: ScriptPosition, radius: ScriptRadius, ms: ScriptLasting| {
                let view = ctx.view();
                let acting = ctx.acting().and_then(|id| view.row(id));
                let team = acting.ok_or_else(|| ApiError::NoActingUnit.fail())?.team;
                Self::reveal(ctx, team, pos.get(), radius, ms)
            },
        )
        .call(
            reveal,
            |ctx: &mut Ctx,
             pos: ScriptPosition,
             radius: ScriptRadius,
             ms: ScriptLasting,
             team: Team| { Self::reveal(ctx, team, pos.get(), radius, ms) },
        )
        .tag_property(TagProperty::Hidden, Status::Runs(ApiVersion::FIRST))
        .tag_property(TagProperty::Detects, Status::Runs(ApiVersion::FIRST))
        .data::<VisionData>(&[]);
    }

    /// Queues a reveal to `team` of the cells within `radius` of `pos`, for `ms`.
    fn reveal(
        ctx: &Ctx,
        team: Team,
        pos: Vec3,
        radius: ScriptRadius,
        ms: ScriptLasting,
    ) -> Checked<()> {
        let radius = radius.get();
        let ticks = ms.get();
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
            ApiOwner::of::<Unit>(),
            "can_see",
            &[&["unit"]],
            "whether its team sees the other unit",
        );
        api.method(can_see, |unit: Unit, other: Unit| {
            VisionColumn::can_see(&unit, &other)
        });
        let visible = MemberSpec::call(
            "find_visible",
            &[&["of", "pos", "radius", "filter"]],
            "as `find`, of the units `of`'s team sees",
        );
        api.call(
            visible,
            |_: &mut Ctx, of: Unit, pos: ScriptPosition, radius: ScriptRadius, filter: Filter| {
                VisionColumn::find(&of, pos.get(), radius, &filter)
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
                VisionColumn::nearest(&of, radius, &filter)
            },
        );
    }
}
