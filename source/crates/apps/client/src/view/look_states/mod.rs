use std::time::Duration;

use bevy::app::{App, Plugin, Update};
use bevy::asset::AssetServer;
use bevy::camera::visibility::Visibility;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::{Added, Allow, Changed, Or};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Local, ParamSet, Query, Res, ResMut};
use bevy::time::Time;
use campfire_sim::{Position, Unpredicted};
use campfire_units::{Destination, Life, Variation};
use campfire_values::{Team, UnitTags, UnitType};

use crate::view::drawing::{Drawing, DrawingOf, Drawn};
use crate::view::match_colors::Paint;
use crate::view::model_scenes::ModelScenes;
use crate::view::unit_looks::{DrawStates, LookDraws, UnitFacts, UnitLooks};
use crate::view::{DrawSystems, ViewSystems};

/// Draws each unit of a look of states in the state of each of its draws whose conditions best
/// match its own, which it picks again as what its conditions test changes: its life, its tags,
/// its destination, and whether it moved of late (animated-looks.md, A look is states of
/// conditions). A state of other models than the last spawns them hidden, and they replace the
/// last as they are dressed, so the unit is never drawn of none.
#[derive(Debug)]
pub(crate) struct LookStatesPlugin;

/// On the root of a unit's drawing of a look of draws: its type and variation, whose draws it
/// draws; the tested conditions its unit held when its states were last picked, none before the
/// first; when its unit last moved; and each draw's state and the models it spawned of it.
#[derive(Component, Debug)]
pub(crate) struct LookStates {
    unit_type: UnitType,
    variation: Option<Variation>,
    held: Option<u64>,
    moved: Duration,
    draws: Vec<PickedState>,
}

/// A draw's state as a drawing draws it: the state, and the models spawned of it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PickedState {
    state: usize,
    models: Vec<Entity>,
}

/// On a model spawned hidden for a new state: the model of the state before, which goes as this
/// one is dressed and shown.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Replaces(pub(crate) Entity);

/// On a kept model of a new state, whose hidden nodes changed, which its dressing hides again:
/// once it is dressed, or as it is dressed.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Redress;

/// The units whose conditions' parts changed: their life, their tags or their destination.
type Retested<'w, 's> = Query<
    'w,
    's,
    &'static Drawing,
    (
        Or<(Changed<Life>, Changed<UnitTags>, Changed<Destination>)>,
        Allow<Unpredicted>,
    ),
>;

/// The new drawings of look states, and every drawing of them.
type Roots<'w, 's> = ParamSet<
    'w,
    's,
    (
        Query<'static, 'static, Entity, Added<LookStates>>,
        Query<'static, 'static, (&'static DrawingOf, &'static mut LookStates)>,
    ),
>;

/// The units whose place changed, which moved.
type Moved<'w, 's> = Query<'w, 's, &'static Drawing, (Changed<Position>, Allow<Unpredicted>)>;

/// What a unit holds that its look's tests read, and its team, which paints its models.
type UnitParts<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Life>,
        &'static UnitTags,
        Option<&'static Destination>,
        &'static Team,
    ),
    Allow<Unpredicted>,
>;

impl Plugin for LookStatesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, Self::pick.after(DrawSystems).in_set(ViewSystems));
    }
}

impl LookStates {
    /// The states of a unit of `unit_type` and `variation` that draws `draws`, none picked yet.
    pub(crate) fn new(
        unit_type: UnitType,
        variation: Option<Variation>,
        draws: &LookDraws,
    ) -> Self {
        Self {
            unit_type,
            variation,
            held: None,
            moved: Duration::ZERO,
            draws: vec![
                PickedState {
                    state: 0,
                    models: Vec::new(),
                };
                draws.draws().len()
            ],
        }
    }
}

impl LookStatesPlugin {
    /// Picks the states of each new drawing, of each unit whose conditions' parts changed or
    /// that moved, and of each that held a condition of its moving as it may stand now; and
    /// spawns the models of each state picked anew.
    fn pick(
        (time, looks): (Res<'_, Time>, Res<'_, UnitLooks>),
        (assets, mut scenes): (Res<'_, AssetServer>, ResMut<'_, ModelScenes>),
        (retested, moved): (Retested<'_, '_>, Moved<'_, '_>),
        units: UnitParts<'_, '_>,
        (replacing, mut roots): (Query<'_, '_, &Replaces>, Roots<'_, '_>),
        (mut walking, mut visits): (Local<'_, Vec<Entity>>, Local<'_, Vec<Entity>>),
        mut commands: Commands<'_, '_>,
    ) {
        let now = time.elapsed();
        visits.clear();
        visits.extend(roots.p0().iter());
        visits.extend(retested.iter().map(Drawing::root));
        let mut roots = roots.p1();
        for drawing in &moved {
            if let Ok((_, mut states)) = roots.get_mut(drawing.root()) {
                states.moved = now;
                visits.push(drawing.root());
            }
        }
        visits.append(&mut walking);
        visits.sort_unstable();
        visits.dedup();
        for &root in &*visits {
            let Ok((&DrawingOf(unit), mut states)) = roots.get_mut(root) else {
                continue;
            };
            let (Ok((life, &tags, destination, &team)), Some(draws), Some(tests)) = (
                units.get(unit),
                looks.draws(states.unit_type, states.variation),
                looks.tests(states.unit_type),
            ) else {
                continue;
            };
            let facts = UnitFacts {
                tags: tags.tags,
                walks: destination.is_some_and(|destination| destination.get().is_some()),
                still_for: now.saturating_sub(states.moved),
                life: life.map(|life| [life.current(), life.max()]),
            };
            let held = tests.held(facts);
            if held & tests.moving() != 0 {
                walking.push(root);
            }
            if states.held == Some(held) {
                continue;
            }
            let first = states.held.is_none();
            states.held = Some(held);
            for (draw, picked) in draws.draws().iter().zip(&mut states.draws) {
                let state = draw.best(held);
                if !first && state == picked.state {
                    continue;
                }
                let mut swap = Swap {
                    root,
                    paint: Paint::of(team),
                    scenes: &mut scenes,
                    assets: &assets,
                };
                swap.to(draw, state, picked, &replacing, &mut commands);
            }
        }
    }
}

/// A change of a draw's state on the drawing at `root`, whose models wear `paint`, their scenes of
/// `scenes`, loaded through `assets`.
#[derive(Debug)]
struct Swap<'a> {
    root: Entity,
    paint: Paint,
    scenes: &'a mut ModelScenes,
    assets: &'a AssetServer,
}

impl Swap<'_> {
    /// Draws `picked`, a state of `draw`, in its state `state`: a model of the same scene as the
    /// one at its place before is kept and takes its new hidden nodes, and any other is spawned,
    /// shown at once at a place of no model before, as of a first state, else hidden to replace
    /// the one before as it is dressed, or the one that one replaces when that one is not shown
    /// yet; a model at a place the new state has none goes, and the one it replaces with it.
    fn to(
        &mut self,
        draw: &DrawStates,
        state: usize,
        picked: &mut PickedState,
        replacing: &Query<'_, '_, &Replaces>,
        commands: &mut Commands<'_, '_>,
    ) {
        let before = &draw.states()[picked.state].models;
        let after = &draw.states()[state].models;
        let mut models = Vec::with_capacity(after.len());
        for (at, model) in after.iter().enumerate() {
            let old = picked.models.get(at).copied();
            let kept = old.filter(|_| {
                before
                    .get(at)
                    .is_some_and(|before| before.same_scene(model))
            });
            if let Some(kept) = kept {
                commands.entity(kept).insert((model.nodes.clone(), Redress));
                models.push(kept);
                continue;
            }
            // A model not shown yet goes now, and the new one replaces what it replaced.
            let replaced = old.map(|old| match replacing.get(old) {
                Ok(&Replaces(shown)) => {
                    commands.entity(old).despawn();
                    shown
                }
                Err(_) => old,
            });
            let bundle = model.bundle(self.scenes, self.assets, self.paint);
            let mut spawned = commands.spawn((bundle, Drawn, ChildOf(self.root)));
            if let Some(shown) = replaced {
                spawned.insert((Visibility::Hidden, Replaces(shown)));
            }
            models.push(spawned.id());
        }
        // A model at a place the new state has none goes, and the one it replaces with it.
        for &gone in picked.models.get(after.len()..).unwrap_or_default() {
            if let Ok(&Replaces(shown)) = replacing.get(gone) {
                commands.entity(shown).despawn();
            }
            commands.entity(gone).despawn();
        }
        *picked = PickedState { state, models };
    }
}

#[cfg(test)]
mod tests;
