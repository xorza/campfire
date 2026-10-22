use campfire_math::Vec3;
use campfire_script::{
    ApiBuilder, ApiError, ApiHolder, ApiVersion, Checked, Ctx, MemberDecl, ScriptLasting,
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
        let reveal = MemberDecl::call(
            "reveal",
            &[
                &["position", "radius", "ms"],
                &["position", "radius", "ms", "team"],
            ],
            "shows `team`'s vision group, else the acting unit's, the cells within `radius` of `position` for `ms`, from this tick's Vision stage; no hidden unit",
        );
        api.call(
            reveal,
            |ctx: &mut Ctx, position: ScriptPosition, radius: ScriptRadius, ms: ScriptLasting| {
                let view = ctx.view();
                let acting = ctx.acting().and_then(|id| view.row(id));
                let team = acting.ok_or_else(|| ApiError::NoActingUnit.fail())?.team;
                Self::reveal(ctx, team, position.get(), radius, ms)
            },
        )
        .call(
            reveal,
            |ctx: &mut Ctx,
             position: ScriptPosition,
             radius: ScriptRadius,
             ms: ScriptLasting,
             team: Team| { Self::reveal(ctx, team, position.get(), radius, ms) },
        )
        .tag_property(TagProperty::Hidden, Status::Runs(ApiVersion::FIRST))
        .tag_property(TagProperty::Detects, Status::Runs(ApiVersion::FIRST))
        .data::<VisionData>(&[]);
    }

    /// Queues a reveal to `team` of the cells within `radius` of `position`, for `ms`.
    fn reveal(
        ctx: &Ctx,
        team: Team,
        position: Vec3,
        radius: ScriptRadius,
        ms: ScriptLasting,
    ) -> Checked<()> {
        let radius = radius.get();
        let ticks = ms.get();
        ctx.queue(VisionEffect {
            team,
            position,
            radius,
            ticks,
        })
    }

    /// `unit.can_see`.
    fn register_queries(api: &mut ApiBuilder<'_>) {
        let can_see = MemberDecl::method(
            ApiHolder::of::<Unit>(),
            "can_see",
            &[&["unit"]],
            "whether its team sees the other unit",
        );
        api.method(can_see, |unit: Unit, other: Unit| {
            VisionColumn::can_see(&unit, &other)
        });
    }
}
