use campfire_math::Num;
use campfire_script::rhai::INT;
use campfire_sim::Capability;

use crate::progression::progression_column::ProgressionColumn;
use crate::progression::progression_effect::ProgressionEffect;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::hook::Hook;
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_api::status::Status;
use crate::units::unit::Unit;

/// The script API of `progression`: `ctx.add_xp` and `on_level_up`; reads of a unit's progress
/// and points; and perks, which the release plans.
#[derive(Debug)]
pub(crate) struct ProgressionApi;

impl ProgressionApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let call = |name, signature, description| {
            MemberSpec::call(name, signature, description).capability(Capability::Progression)
        };
        let unit = |name, description| {
            MemberSpec::field(ApiOwner::Unit, name, description).capability(Capability::Progression)
        };
        let method = |name, signature, description| {
            MemberSpec::method(ApiOwner::Unit, name, signature, description)
                .capability(Capability::Progression)
        };
        let add_xp = call(
            "add_xp",
            &[&["unit", "track", "amount"]],
            "gives `unit` `amount` of experience on `track`, one of its unit type's",
        )
        .name(1, NameKind::Track);
        api.bind(
            add_xp,
            |ctx: &mut Ctx, unit: Unit, track: &str, amount: Num| {
                ProgressionApi::add_xp(ctx, &unit, track, amount)
            },
        )
        .bind(
            add_xp,
            |ctx: &mut Ctx, unit: Unit, track: &str, amount: INT| {
                ProgressionApi::add_xp(ctx, &unit, track, ApiError::num(amount)?)
            },
        )
        .plan(call(
            "grant_perk",
            &[&["unit", "id"]],
            "gives `unit` the perk `id`, with no point and no requirement",
        ))
        .bind(
            method(
                "xp",
                &[&["track"]],
                "its experience on `track`, one of its unit type's",
            )
            .name(0, NameKind::Track),
            |unit: Unit, track: &str| {
                let view = unit.view();
                let track = ProgressionColumn::track_named(view, track)?;
                ProgressionColumn::xp(view, unit.row_index(), track)
            },
        )
        .bind(
            method(
                "track_level",
                &[&["track"]],
                "its level on `track`, one of its unit type's",
            )
            .name(0, NameKind::Track),
            |unit: Unit, track: &str| -> Checked<INT> {
                let view = unit.view();
                let track = ProgressionColumn::track_named(view, track)?;
                let level = ProgressionColumn::level(view, unit.row_index(), track)?;
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
        .data(DataTable::Mode, &["tracks"], &[])
        .data(DataTable::Track, &["levels", "level"], &[]);
    }

    /// Queues `amount`, not negative, of experience on `track` of `unit`, which has it.
    fn add_xp(ctx: &Ctx, unit: &Unit, track: &str, amount: Num) -> Checked<()> {
        let view = ctx.view();
        let track = ProgressionColumn::track_named(view, track)?;
        if amount < Num::ZERO {
            return Err(ApiError::NegativeXp.fail().into());
        }
        if !ProgressionColumn::has(view, unit.id, track) {
            return Err(ApiError::NoTrack.fail().into());
        }
        ctx.queue(ProgressionEffect::AddXp {
            unit: unit.id,
            track,
            amount,
        })
    }
}
