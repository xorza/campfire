use campfire_math::Num;
use campfire_script::rhai::INT;

use crate::progression::progression_column::ProgressionColumn;
use crate::progression::progression_effect::ProgressionEffect;
use crate::progression::track_data::TrackData;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::hook::Hook;

use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_api::status::Status;
use crate::units::track_id::TrackId;
use crate::units::unit::Unit;

/// The script API of `progression`: `ctx.add_xp` and `on_level_up`; reads of a unit's progress
/// and points; and perks, which the release plans.
#[derive(Debug)]
pub(crate) struct ProgressionApi;

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
                ProgressionApi::add_xp(ctx, &unit, track, amount)
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
            |unit: Unit, track: TrackId| -> Checked<INT> {
                let level = ProgressionColumn::level(unit.view(), unit.row_index(), track)?;
                Ok(INT::from(level.get()))
            },
        )
        .bind(
            unit(
                "points",
                "its unspent points, which a unit with the `level` track has",
            ),
            |unit: &mut Unit| -> Checked<INT> {
                let points = ProgressionColumn::points(unit.view(), unit.row_index())?;
                Ok(INT::from(points.get()))
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
        let view = ctx.view();
        if amount < Num::ZERO {
            return Err(ApiError::NegativeXp.fail().into());
        }
        if !ProgressionColumn::has(view, unit.id, track) {
            return Err(ApiError::NoTrack.fail().into());
        }
        ctx.queue(ProgressionEffect::AddXp {
            id: unit.id,
            track,
            amount,
        })
    }
}
