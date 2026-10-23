use campfire_script::{ApiBuilder, ApiHolder, Checked, MemberDecl, Unit};
use campfire_values::TrackId;
use campfire_visibility::DataKind;

use crate::progression_column::ProgressionColumn;
use crate::track_data::TrackData;

/// The script API of `progression` beside its effect's call: `on_level_up`; reads of a unit's
/// progress and points; and perks, which the release plans.
#[derive(Debug)]
pub struct ProgressionApi;

impl ProgressionApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let call = |name, signature, description| MemberDecl::call(name, signature, description);
        let unit =
            |name, description| MemberDecl::field(ApiHolder::of::<Unit>(), name, description);
        let method = |name, signature, description| {
            MemberDecl::method(ApiHolder::of::<Unit>(), name, signature, description)
        };
        api.plan(call(
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
        .plan(method(
            "has_perk",
            &[&["id"]],
            "whether it has the perk `id`",
        ))
        .data::<TrackData>(&[]);
    }
}
