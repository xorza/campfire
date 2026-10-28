use campfire_common::PlayerSlot;
use campfire_script::{
    ApiBuilder, ApiHolder, CapabilityRefusal, Checked, Ctx, Frame, MemberDecl, PlayerHandle, Unit,
};
use campfire_values::{PerkId, TrackId};
use campfire_visibility::DataKind;

use crate::error::ProgressionRefusal;
use crate::perk_data::PerkData;
use crate::progression_call::ProgressionCall;
use crate::progression_column::ProgressionColumn;
use crate::progression_effect::ProgressionEffect;
use crate::track_data::TrackData;

/// The script API of `progression` beside its effect's call: `on_level_up`; reads of a unit's
/// progress and points; a perk the mode grants; and reads of a player's perks and points.
#[derive(Debug)]
pub struct ProgressionApi;

impl ProgressionApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let unit =
            |name, description| MemberDecl::field(ApiHolder::of::<Unit>(), name, description);
        let method = |name, signature, description| {
            MemberDecl::method(ApiHolder::of::<Unit>(), name, signature, description)
        };
        api.call(
            MemberDecl::mode_call(
                "grant_perk",
                &[&["player", "id"]],
                "gives `player` the perk `id`, with no point and no requirement, and its modifier",
            ),
            |ctx: &mut Ctx, player: PlayerSlot, perk: PerkId| {
                ctx.queue(ProgressionEffect::GrantPerk { player, perk })
            },
        )
        .method(
            method(
                "xp",
                &[&["track"]],
                "its experience on `track`, one of its unit type's",
            ),
            |unit: Unit, track: TrackId| {
                ProgressionColumn::xp(unit.view(), unit.admit(DataKind::Progression)?, track)
            },
        )
        .method(
            method(
                "track_level",
                &[&["track"]],
                "its level on `track`, one of its unit type's",
            ),
            |unit: Unit, track: TrackId| -> Checked<i64> {
                let level = ProgressionColumn::level(
                    unit.view(),
                    unit.admit(DataKind::Progression)?,
                    track,
                )?;
                Ok(i64::from(level.get()))
            },
        )
        .bind(
            unit(
                "points",
                "its unspent points, which a unit with the `level` track has",
            ),
            |unit: &mut Unit| -> Checked<i64> {
                let points =
                    ProgressionColumn::points(unit.view(), unit.admit(DataKind::Progression)?)?;
                Ok(i64::from(points.get()))
            },
        )
        .method(
            MemberDecl::method(
                ApiHolder::of::<PlayerHandle>(),
                "has_perk",
                &[&["id"]],
                "whether it has the perk `id`",
            ),
            |ctx: &Ctx, player: PlayerHandle, perk: PerkId| -> Checked<bool> {
                let player = player.player_slot();
                ctx.view().admit_player(player, DataKind::Progression)?;
                let frame = ctx.frame();
                Ok(Self::call(&frame).has_perk(player, perk))
            },
        )
        .bind(
            MemberDecl::field(
                ApiHolder::of::<PlayerHandle>(),
                "perk_points",
                "its unspent points to learn perks, which a player of a track of points has",
            ),
            |ctx: &Ctx, player: &mut PlayerHandle| -> Checked<i64> {
                let player = player.player_slot();
                ctx.view().admit_player(player, DataKind::Progression)?;
                let frame = ctx.frame();
                let points = Self::call(&frame)
                    .points(player)
                    .ok_or_else(|| ProgressionRefusal::NoPoints.fail())?;
                Ok(i64::from(points))
            },
        )
        .data::<TrackData>(&[])
        .data::<PerkData>(&[]);
    }

    /// The running call's part of progression, of its frame `frame`.
    fn call(frame: &Frame) -> &ProgressionCall {
        frame
            .extension::<ProgressionCall>()
            .expect("every call holds progression's")
    }
}
