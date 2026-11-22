use bevy::app::{App, Plugin, Startup, Update};
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::Allow;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res};
use bevy::time::Time;
use campfire_names::MatchNames;
use campfire_sim::Unpredicted;
use campfire_values::{Tag, TagSet, UnitTags};

use crate::view::drawing::DrawingOf;
use crate::view::match_colors::{MatchColors, Paint, Tint};
use crate::view::match_hud::MatchHud;
use crate::view::{DrawSystems, ViewSystems};

/// Marks each drawn unit by the tags it holds, as the HUD file's tag marks say: tints its models,
/// faded in and out, and stills its clips (02-engine-core.md, A tag marks its unit).
#[derive(Debug)]
pub(crate) struct TagMarksPlugin;

/// The match's tag marks, each by its place among the HUD file's: its tag, none for a tag the
/// match lacks, which marks no unit; the seconds of its fade; and whether it stills clips.
#[derive(Resource, Debug, Default)]
pub(crate) struct TagMarks(Vec<ResolvedMark>);

#[derive(Debug, Clone, Copy)]
struct ResolvedMark {
    tag: Option<Tag>,
    fade_seconds: f32,
    stills: bool,
}

/// On the root of a drawing of a unit that holds, or held within its fade, a marked tag: how far
/// each mark has faded in, from 0 to 1, by the marks' places.
#[derive(Component, Debug)]
struct MarkFades(Box<[f32]>);

impl Plugin for TagMarksPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TagMarks>();
        app.add_systems(Startup, Self::resolve);
        app.add_systems(Update, Self::fade.after(DrawSystems).in_set(ViewSystems));
    }
}

impl TagMarksPlugin {
    /// Resolves the tag of each of the HUD file's marks by the match's names.
    fn resolve(
        hud: Option<Res<'_, MatchHud>>,
        names: Option<Res<'_, MatchNames>>,
        mut commands: Commands<'_, '_>,
    ) {
        let (Some(hud), Some(names)) = (hud, names) else {
            return;
        };
        let marks = hud.0.tag_marks.iter().map(|mark| {
            let tag = names.tags().named(mark.tag.as_str());
            if tag.is_none() {
                tracing::error!(
                    tag = mark.tag.as_str(),
                    "a tag mark names no tag of the match"
                );
            }
            #[expect(
                clippy::cast_precision_loss,
                reason = "a fade of a few seconds is exact in f32"
            )]
            let fade_seconds = mark.fade_ms as f32 / 1000.0;
            ResolvedMark {
                tag,
                fade_seconds,
                stills: mark.stills,
            }
        });
        commands.insert_resource(TagMarks(marks.collect()));
    }

    /// Fades each mark of each drawn unit in while it holds the mark's tag and out once it does
    /// not, and tints its models by the first mark faded in at all, at its step of the fade; a
    /// drawing marks nothing once each of its marks faded out.
    fn fade(
        marks: Res<'_, TagMarks>,
        time: Res<'_, Time>,
        mut roots: Query<'_, '_, (Entity, &DrawingOf, Option<&mut MarkFades>, &Children)>,
        units: Query<'_, '_, &UnitTags, Allow<Unpredicted>>,
        mut paints: Query<'_, '_, &mut Paint>,
        mut commands: Commands<'_, '_>,
    ) {
        if marks.0.is_empty() {
            return;
        }
        let seconds = time.delta_secs();
        for (root, &DrawingOf(unit), fades, models) in &mut roots {
            let Ok(tags) = units.get(unit) else {
                continue;
            };
            let tint = match fades {
                Some(mut fades) => {
                    let faded = marks.step(&mut fades.0, tags.tags, seconds);
                    if faded.is_none() && !marks.holds(tags.tags) {
                        commands.entity(root).remove::<MarkFades>();
                    }
                    faded
                }
                None if marks.holds(tags.tags) => {
                    let mut fades = vec![0.0; marks.0.len()].into_boxed_slice();
                    let faded = marks.step(&mut fades, tags.tags, seconds);
                    commands.entity(root).insert(MarkFades(fades));
                    faded
                }
                None => continue,
            };
            for &model in models {
                if let Ok(paint) = paints.get_mut(model) {
                    paint
                        .map_unchanged(|paint| &mut paint.tint)
                        .set_if_neq(tint);
                }
            }
        }
    }
}

impl TagMarks {
    /// Whether `tags` hold the tag of a mark.
    fn holds(&self, tags: TagSet) -> bool {
        self.0
            .iter()
            .any(|mark| mark.tag.is_some_and(|tag| tags.contains(tag)))
    }

    /// Whether a unit of `tags` holds the tag of a mark that stills its clips, as Zero Hour pauses
    /// a disabled object's animations as it is disabled.
    pub(crate) fn stills(&self, tags: TagSet) -> bool {
        (self.0.iter()).any(|mark| mark.stills && mark.tag.is_some_and(|tag| tags.contains(tag)))
    }

    /// Moves each of `fades` by `seconds` of a unit of `tags`: in while it holds its mark's tag,
    /// out while it does not, each over its fade; and gives the tint of the first faded in at all,
    /// at its step, the fade's share rounded up to the colors' steps, none when none is.
    fn step(&self, fades: &mut [f32], tags: TagSet, seconds: f32) -> Option<Tint> {
        debug_assert_eq!(fades.len(), self.0.len(), "a fade of each mark");
        let mut tint = None;
        for (at, (mark, fade)) in (0..=u8::MAX).zip(self.0.iter().zip(fades.iter_mut())) {
            let held = mark.tag.is_some_and(|tag| tags.contains(tag));
            let moved = if mark.fade_seconds > 0.0 {
                seconds / mark.fade_seconds
            } else {
                1.0
            };
            *fade = if held { *fade + moved } else { *fade - moved }.clamp(0.0, 1.0);
            if tint.is_none() && *fade > 0.0 {
                let steps = f32::from(MatchColors::FADE_STEPS);
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a share from 0 to 1 of 30 steps, rounded up"
                )]
                let step = (*fade * steps).ceil() as u8;
                tint = Some(Tint::Mark {
                    mark: at,
                    step: step.clamp(1, MatchColors::FADE_STEPS) - 1,
                });
            }
        }
        tint
    }
}

#[cfg(test)]
mod tests;
