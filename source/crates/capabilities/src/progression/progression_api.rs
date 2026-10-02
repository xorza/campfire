use campfire_math::Num;
use campfire_script::rhai::INT;
use campfire_sim::Capability;

use crate::progression::progression_effect::ProgressionEffect;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::hook::Hook;
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_api::{ApiOwner, DataTable, MemberSpec, Status};
use crate::units::unit::Unit;

/// The script API of `progression`: `ctx.add_xp` and `on_level_up`; reads of a unit's progress,
/// points and perks, which the release plans.
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
            "(unit, track, amount)",
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
            "(unit, id)",
            "gives `unit` the perk `id`, with no point and no requirement",
        ))
        .plan(method("xp", "(track)", "its experience on `track`"))
        .plan(method("track_level", "(track)", "its level on `track`"))
        .plan(unit("points", "its unspent points"))
        .plan(method("has_perk", "(id)", "whether it has the perk `id`"))
        .hook(Hook::OnLevelUp, Status::Runs(ApiVersion::FIRST))
        .data(DataTable::Mode, &["tracks"], &[])
        .data(DataTable::Track, &["levels", "level"], &[]);
    }

    /// Queues `amount`, not negative, of experience on `track` of `unit`, which has it.
    fn add_xp(ctx: &Ctx, unit: &Unit, track: &str, amount: Num) -> Checked<()> {
        let view = ctx.view();
        let track = view.track_named(track)?;
        if amount < Num::ZERO {
            return Err(ApiError::NegativeXp.fail().into());
        }
        if !view
            .row(unit.id)
            .is_some_and(|row| row.tracks.contains(track))
        {
            return Err(ApiError::NoTrack.fail().into());
        }
        ctx.queue(ProgressionEffect::AddXp {
            unit: unit.id,
            track,
            amount,
        })
    }
}
