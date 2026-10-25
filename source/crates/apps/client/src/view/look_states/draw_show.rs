use std::fmt::{self, Debug, Formatter};

use bevy::animation::graph::AnimationGraph;
use bevy::asset::{AssetServer, Assets};
use bevy::camera::visibility::Visibility;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::system::{Commands, Query};
use campfire_client_files::{AnimationMode, StartFrame};

use crate::view::clip_clock::{ClipClock, ClipStart};
use crate::view::drawing::Drawn;
use crate::view::look_clips::{ClipTiming, LookClip, LookClips};
use crate::view::look_states::LookDice;
use crate::view::match_colors::Paint;
use crate::view::model_scenes::ModelScenes;
use crate::view::unit_looks::{DrawStates, LookModel, Showing};
use crate::view::unit_models::{Redress, Replaces};

/// A draw as a drawing shows it, as `W3DModelDraw` holds its current state and its next: what it
/// shows, a state or a transition, none before its first; the state it shows next, once what it
/// shows completes its clip; the models spawned of what it shows; and the clip it plays.
#[derive(Debug, Clone, Default)]
pub(crate) struct DrawShow {
    showing: Option<Showing>,
    pending: Option<usize>,
    models: Vec<Entity>,
    play: Option<ClipPlay>,
}

/// A clip a draw plays: its clip, of its graph; its place among the clips of what the draw shows;
/// the meters its unit walks in one run of it, none for one of its own rate; its mode; its speed,
/// a factor of its rate; how it starts, until its timing loads; and its clock once it has.
#[derive(Debug, Clone)]
pub(crate) struct ClipPlay {
    pub(crate) clip: LookClip,
    at: usize,
    stride: Option<f32>,
    mode: AnimationMode,
    speed: f32,
    start: PlayStart,
    clock: Option<ClipClock>,
}

/// How a clip starts: at a frame its state names, or at the share of its frames the state before
/// it stood at.
#[derive(Debug, Clone, Copy, PartialEq)]
enum PlayStart {
    Frame(StartFrame),
    Kept(f32),
}

/// Where a draw's clip stands, as its models' players pose them: its clip, its timing and its
/// time in seconds; and its frame, by which its pivots are hidden.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClipPose<'a> {
    pub(crate) clip: &'a LookClip,
    pub(crate) timing: &'a ClipTiming,
    pub(crate) seconds: f32,
    pub(crate) frame: u32,
}

/// What a change of what a draw shows reads and spawns through: the drawing's root, its models'
/// paint, its dice, the scenes and clips loaded, the clips' timings, the graphs, the asset server,
/// the models waiting to replace others, and the commands.
pub(crate) struct ShowCx<'a, 'w, 's> {
    pub(crate) root: Entity,
    pub(crate) paint: Paint,
    pub(crate) dice: &'a mut LookDice,
    pub(crate) scenes: &'a mut ModelScenes,
    pub(crate) clips: &'a mut LookClips,
    pub(crate) timings: &'a Assets<ClipTiming>,
    pub(crate) graphs: &'a mut Assets<AnimationGraph>,
    pub(crate) assets: &'a AssetServer,
    pub(crate) replacing: &'a Query<'a, 'a, &'static Replaces>,
    pub(crate) commands: &'a mut Commands<'w, 's>,
}

impl Debug for ShowCx<'_, '_, '_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShowCx")
            .field("root", &self.root)
            .field("paint", &self.paint)
            .field("dice", &self.dice)
            .finish_non_exhaustive()
    }
}

/// A frame's step of a draw's clip: the seconds that passed, and the meters a second its drawing
/// walks, 0 for one that stands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ClipStep {
    pub(crate) seconds: f32,
    pub(crate) speed: f32,
}

/// What a draw showed as it starts what it shows: that, none before its first; and the share
/// of its frames its clip stood at, kept across states, none for a clip that keeps none.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Before {
    showing: Option<Showing>,
    kept: Option<f32>,
}

impl DrawShow {
    pub(crate) fn models(&self) -> &[Entity] {
        &self.models
    }

    /// Asks `draw` to show its state `state`, as `W3DModelDraw::setModelState` does: no change
    /// for the state it shows of none next, or the one it shows next; the state shown next when
    /// it lets the shown one finish its clip; the transition between the two shown first, when
    /// both have keys and the draw has one of them; else the state at once.
    pub(crate) fn request(&mut self, draw: &DrawStates, state: usize, cx: &mut ShowCx<'_, '_, '_>) {
        let showing = Showing::State(state);
        let Some(shown) = self.showing else {
            self.show(draw, showing, cx);
            return;
        };
        if (shown == showing && self.pending.is_none()) || self.pending == Some(state) {
            return;
        }
        let key = match shown {
            Showing::State(at) => draw.states()[at].key,
            Showing::Transition(_) => None,
        };
        let wanted = &draw.states()[state];
        if shown != showing {
            if wanted.waits_for.is_some() && wanted.waits_for == key && !self.complete() {
                self.pending = Some(state);
                return;
            }
            if let (Some(from), Some(to)) = (key, wanted.key)
                && let Some(transition) = draw.transition(from, to)
            {
                self.pending = Some(state);
                self.show(draw, Showing::Transition(transition), cx);
                return;
            }
        }
        self.pending = None;
        self.show(draw, showing, cx);
    }

    /// Moves its clip by `step`, as `W3DModelDraw::doDrawModule` does before its model is drawn:
    /// once its clip completed, it shows the state it shows next, and then draws another idle
    /// clip, or starts again a clip of a state that says so; a clip of a stride takes the rate
    /// that walks it at its unit's speed; and a clip that just started stands at its start frame,
    /// which a frame draws before the clip moves, as `Set_Animation` starts it.
    pub(crate) fn advance(
        &mut self,
        draw: &DrawStates,
        step: ClipStep,
        cx: &mut ShowCx<'_, '_, '_>,
    ) {
        if self.complete() {
            if let Some(next) = self.pending.take() {
                self.request(draw, next, cx);
            }
            if let Some(shown) = self.showing
                && let Some(animation) = &draw.shown(shown).animation
                && (animation.idle || animation.restart)
            {
                let before = Before {
                    showing: Some(shown),
                    kept: None,
                };
                self.start_clip(draw, before, cx);
            }
        }
        let Some(play) = &mut self.play else {
            return;
        };
        let Some(timing) = cx.timings.get(&play.clip.timing) else {
            return;
        };
        if let Some(stride) = play.stride
            && step.speed > 0.0
        {
            play.speed = timing.walked(stride, step.speed);
        }
        if let Some(clock) = &mut play.clock {
            clock.advance(step.seconds * timing.rate as f32 * play.speed);
        } else {
            let start = play.start.clip_start(timing.frames, cx.dice);
            play.clock = Some(ClipClock::start(timing.frames, play.mode, start));
        }
    }

    /// Where its clip stands, its timing of `timings`; none before its timing loads.
    #[expect(
        clippy::cast_sign_loss,
        reason = "a clip's frame is never below 0, and W3D truncates it, as `Get_Bit((int)frame)`"
    )]
    pub(crate) fn pose<'a>(&'a self, timings: &'a Assets<ClipTiming>) -> Option<ClipPose<'a>> {
        let play = self.play.as_ref()?;
        let clock = play.clock.as_ref()?;
        let timing = timings.get(&play.clip.timing)?;
        Some(ClipPose {
            clip: &play.clip,
            timing,
            seconds: clock.frame() / timing.rate as f32,
            frame: clock.frame() as u32,
        })
    }

    /// Whether its clip completed, as `isAnimationComplete` says; not for no clip.
    fn complete(&self) -> bool {
        let clock = self.play.as_ref().and_then(|play| play.clock);
        clock.is_some_and(|clock| clock.is_complete())
    }

    /// Shows `showing` of `draw`: its models, then its clip, which keeps the share its clip
    /// before stood at when both keep frames across a group in common.
    fn show(&mut self, draw: &DrawStates, showing: Showing, cx: &mut ShowCx<'_, '_, '_>) {
        let before = Before {
            showing: self.showing,
            kept: self
                .showing
                .and_then(|before| draw.shown(before).animation.as_ref())
                .filter(|animation| animation.maintain != 0)
                .and(self.play.as_ref())
                .and_then(|play| play.clock)
                .map(|clock| clock.share()),
        };
        let none: &[LookModel] = &[];
        let old = before
            .showing
            .map_or(none, |before| &draw.shown(before).models);
        let all_kept = cx.swap(old, &draw.shown(showing).models, &mut self.models);
        self.showing = Some(showing);
        if draw.shown(showing).animation.is_none() && !all_kept {
            self.play = None;
        }
        self.start_clip(draw, before, cx);
    }

    /// Starts the clip of what it shows, as `W3DModelDraw::adjustAnimation` does, `before` the
    /// one it showed: one drawn of its clips, of idle clips another than the one it played; at
    /// its state's start, or at `kept`, the share the clip before stood at, when the two keep
    /// frames across a group in common; at a speed drawn from its range. What shows no clip plays
    /// on the clip it played.
    fn start_clip(&mut self, draw: &DrawStates, before: Before, cx: &mut ShowCx<'_, '_, '_>) {
        let Before {
            showing: before,
            kept,
        } = before;
        let shown = self.showing.expect("a clip starts as a draw shows");
        let Some(animation) = &draw.shown(shown).animation else {
            return;
        };
        let count = animation.clips.len();
        let at = match (&self.play, before == Some(shown)) {
            _ if count == 1 => 0,
            (Some(play), true) => loop {
                let at = cx.dice.below(count);
                if at != play.at {
                    break at;
                }
            },
            _ => cx.dice.below(count),
        };
        let maintained = before
            .filter(|&before| before != shown)
            .and_then(|before| draw.shown(before).animation.as_ref())
            .is_some_and(|previous| previous.maintain & animation.maintain != 0);
        let start = match (animation.start, kept) {
            (StartFrame::Natural, Some(share)) if maintained => PlayStart::Kept(share),
            (start, _) => PlayStart::Frame(start),
        };
        let file = &animation.clips[at];
        let [least, most] = animation.speed;
        self.play = Some(ClipPlay {
            clip: cx.clips.clip(&file.path, cx.assets, cx.graphs),
            at,
            stride: file.stride,
            mode: animation.mode,
            speed: cx.dice.between(least, most),
            start,
            clock: None,
        });
    }
}

impl PlayStart {
    /// The start of a clip of `frames` frames, a random frame drawn of `dice`.
    fn clip_start(self, frames: u32, dice: &mut LookDice) -> ClipStart {
        match self {
            Self::Frame(StartFrame::Natural) => ClipStart::Natural,
            Self::Frame(StartFrame::First) => ClipStart::First,
            Self::Frame(StartFrame::Last) => ClipStart::Last,
            Self::Frame(StartFrame::Random) => {
                let frame = dice.below(usize::try_from(frames).expect("a u32 fits usize"));
                ClipStart::At(u32::try_from(frame).expect("below a u32"))
            }
            Self::Kept(share) => ClipStart::Kept(share),
        }
    }
}

impl ShowCx<'_, '_, '_> {
    /// Spawns `after`, the models of what a draw shows now, in place of `before`, those of what it
    /// showed, the entities of which `models` holds: a model of the same scene as the one at its
    /// place before is kept and takes its new hidden nodes, and any other is spawned, shown at
    /// once at a place of no model before, else hidden to replace the one before as it is
    /// dressed, or the one that one replaces when that one is not shown yet; a model at a place
    /// the new state has none goes, and the one it replaces with it. Whether every model was
    /// kept.
    fn swap(
        &mut self,
        before: &[LookModel],
        after: &[LookModel],
        models: &mut Vec<Entity>,
    ) -> bool {
        let cx = self;
        let mut spawned_any = false;
        let mut next = Vec::with_capacity(after.len());
        for (at, model) in after.iter().enumerate() {
            let old = models.get(at).copied();
            let kept = old.filter(|_| {
                before
                    .get(at)
                    .is_some_and(|before| before.same_scene(model))
            });
            if let Some(kept) = kept {
                cx.commands
                    .entity(kept)
                    .insert((model.nodes.clone(), Redress));
                next.push(kept);
                continue;
            }
            spawned_any = true;
            // A model not shown yet goes now, and the new one replaces what it replaced.
            let replaced = old.map(|old| match cx.replacing.get(old) {
                Ok(&Replaces(shown)) => {
                    cx.commands.entity(old).despawn();
                    shown
                }
                Err(_) => old,
            });
            let bundle = model.bundle(cx.scenes, cx.assets, cx.paint);
            let mut spawned = cx.commands.spawn((bundle, Drawn, ChildOf(cx.root)));
            if let Some(shown) = replaced {
                spawned.insert((Visibility::Hidden, Replaces(shown)));
            }
            next.push(spawned.id());
        }
        for &gone in models.get(after.len()..).unwrap_or_default() {
            if let Ok(&Replaces(shown)) = cx.replacing.get(gone) {
                cx.commands.entity(shown).despawn();
            }
            cx.commands.entity(gone).despawn();
        }
        *models = next;
        !spawned_any
    }
}

#[cfg(test)]
pub(crate) mod reach_in {
    use super::*;

    impl DrawShow {
        pub(crate) const fn showing(&self) -> Option<Showing> {
            self.showing
        }

        pub(crate) const fn pending(&self) -> Option<usize> {
            self.pending
        }

        /// The place of the clip it plays among what it shows, and that clip's frame, none
        /// before its timing loads.
        pub(crate) fn playing(&self) -> Option<(usize, Option<f32>)> {
            let play = self.play.as_ref()?;
            Some((play.at, play.clock.map(|clock| clock.frame())))
        }
    }
}
