use campfire_math::Num;
use campfire_script::{
    ApiBuilder, ApiOwner, ApiVersion, CapabilityRefusal, Checked, Ctx, DataTable, Hook, MemberSpec,
    Status, Unit,
};
use campfire_values::TrackId;

use crate::progression_column::ProgressionColumn;
use crate::progression_effect::ProgressionEffect;
use crate::track_data::TrackData;

/// The script API of `progression`: `ctx.add_xp` and `on_level_up`; reads of a unit's progress
/// and points; and perks, which the release plans.
#[derive(Debug)]
pub struct ProgressionApi;

impl ProgressionApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let call = |name, signature, description| MemberSpec::call(name, signature, description);
        let unit = |name, description| MemberSpec::field(ApiOwner::Unit, name, description);
        let method = |name, signature, description| {
            MemberSpec::method(ApiOwner::Unit, name, signature, description)
        };
        let add_xp = call(
            "add_xp",
            &[&["unit", "track", "amount"]],
            "gives `unit` `amount` of experience on `track`, one of its unit type's",
        );
        api.call(
            add_xp,
            |ctx: &mut Ctx, unit: Unit, track: TrackId, amount: Num| {
                Self::add_xp(ctx, &unit, track, amount)
            },
        )
        .plan(call(
            "grant_perk",
            &[&["unit", "id"]],
            "gives `unit` the perk `id`, with no point and no requirement",
        ))
        .method(
            method(
                "xp",
                &[&["track"]],
                "its experience on `track`, one of its unit type's",
            ),
            |unit: Unit, track: TrackId| {
                ProgressionColumn::xp(unit.view(), unit.row_index(), track)
            },
        )
        .method(
            method(
                "track_level",
                &[&["track"]],
                "its level on `track`, one of its unit type's",
            ),
            |unit: Unit, track: TrackId| -> Checked<i64> {
                let level = ProgressionColumn::level(unit.view(), unit.row_index(), track)?;
                Ok(i64::from(level.get()))
            },
        )
        .bind(
            unit(
                "points",
                "its unspent points, which a unit with the `level` track has",
            ),
            |unit: &mut Unit| -> Checked<i64> {
                let points = ProgressionColumn::points(unit.view(), unit.row_index())?;
                Ok(i64::from(points.get()))
            },
        )
        .plan(method(
            "has_perk",
            &[&["id"]],
            "whether it has the perk `id`",
        ))
        .hook(Hook::OnLevelUp, Status::Runs(ApiVersion::FIRST))
        .shared_data(DataTable::Mode, &["tracks"], &[])
        .shared_data(DataTable::UnitType, &["tracks"], &[])
        .data::<TrackData>(&["levels", "level"], &[]);
    }

    /// Queues `amount`, not negative, of experience on `track` of `unit`, which has it.
    fn add_xp(ctx: &Ctx, unit: &Unit, track: TrackId, amount: Num) -> Checked<()> {
        let effect = ProgressionEffect::add_xp(ctx.view(), unit.id, track, amount);
        ctx.queue(effect.map_err(CapabilityRefusal::fail)?)
    }
}
