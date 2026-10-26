use std::time::Duration;

use bevy::animation::AnimationPlayer;
use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle};
use bevy::app::{App, Plugin, Update};
use bevy::asset::{AssetApp, AssetServer, Assets};
use bevy::camera::visibility::Visibility;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::ecs::query::{Added, Allow, Changed, Has, Or};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Local, ParamSet, Query, Res, ResMut};
use bevy::time::{Fixed, Time};
use bevy::transform::components::Transform;
use campfire_actions::{ActionProgress, ActionSlots, SlotTimes};
use campfire_client_files::{FireBarrel, SiteStage};
use campfire_combat::{ClipShots, Weapon};
use campfire_declarations::ById;
use campfire_math::Num;
use campfire_production::{Site, SiteProgress};
use campfire_sim::{Position, SimTick, Unpredicted};
use campfire_units::{Dead, Destination, Life, Variation};
use campfire_values::{ActionId, Team, UnitTags, UnitType};

use crate::view::drawing::{Drawing, DrawingOf};
use crate::view::float_num::FloatNum;
use crate::view::glide::{FrameClock, Glide};
use crate::view::look_clips::{ClipHidden, ClipPivot, ClipTiming, LookClips};
use crate::view::look_states::draw_show::{ClipPose, ClipStep, DrawShow, Relacked, ShowCx};
use crate::view::match_colors::Paint;
use crate::view::model_scenes::ModelScenes;
use crate::view::slot_shots::{AttackParts, ClipLack, SlotShots};
use crate::view::unit_looks::{LookDraws, UnitFacts, UnitLooks};
use crate::view::unit_models::{Dressed, Replaces};
use crate::view::{DrawSystems, ViewSystems};

pub(crate) mod draw_show;

/// Draws each unit of a look of states in the state of each of its draws whose conditions best
/// match its own, which it picks again as what its conditions test changes: its life, its tags,
/// its destination, its site, its attack, and whether it moved of late (02-engine-core.md, A look
/// is states of conditions); and plays each state's clip by its mode, through a transition between two states
/// of one, and its idle clips one after another (02-engine-core.md, A state plays its animation by
/// its mode). A state of other models than the last spawns them hidden, and they replace the last
/// as they are dressed, so the unit is never drawn of none.
#[derive(Debug)]
pub(crate) struct LookStatesPlugin;

/// On the root of a unit's drawing of a look of draws: its type and variation, whose draws it
/// draws; the tested conditions its unit held when its states were last picked, none before the
/// first; the rounds its weapon groups' clips lacked then, by the groups' places; when its unit
/// last moved; the shots of its slots it saw; its dice; and how it shows each of its draws.
#[derive(Component, Debug)]
pub(crate) struct LookStates {
    unit_type: UnitType,
    variation: Option<Variation>,
    held: Option<u64>,
    lacks: Vec<u16>,
    moved: Duration,
    shots: SlotShots,
    dice: LookDice,
    draws: Vec<DrawShow>,
}

/// The dice a drawing draws its clips, their starts and their speeds of, as Zero Hour's client
/// draws them of `GameClientRandomValue`: SplitMix64, of a seed of its unit, which no sim reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LookDice(u64);

/// The units whose conditions' parts changed: their life, their tags or their destination.
type Retested<'w, 's> = Query<
    'w,
    's,
    &'static Drawing,
    (
        Or<(
            Changed<Life>,
            Changed<UnitTags>,
            Changed<Destination>,
            Changed<SiteProgress>,
            Changed<ActionProgress>,
            Changed<SlotTimes>,
            Changed<ClipShots>,
            Added<Dead>,
        )>,
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

/// What the pick keeps between its runs: the drawings a condition time ends visits again, the
/// drawings it visits, and what a unit's clips lack.
type PickScratch<'s> = (
    Local<'s, Vec<Entity>>,
    Local<'s, Vec<Entity>>,
    Local<'s, Vec<ClipLack>>,
);

/// The units whose place changed, which moved.
type Moved<'w, 's> = Query<'w, 's, &'static Drawing, (Changed<Position>, Allow<Unpredicted>)>;

/// What a unit holds that its look's tests read, its site, which raises a state's models as it is
/// built, and its team, which paints its models.
type UnitParts<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Life>,
        &'static UnitTags,
        Option<&'static Destination>,
        Option<(&'static Site, &'static SiteProgress)>,
        Has<Dead>,
        &'static Team,
    ),
    Allow<Unpredicted>,
>;

/// What a unit holds that its attack's phase reads: its progress and slots, and its slots' times
/// and its clips' shots, where its client holds them.
type AttackQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static ActionProgress,
        &'static ActionSlots,
        Option<&'static SlotTimes>,
        Option<&'static ClipShots>,
    ),
    Allow<Unpredicted>,
>;

/// What a pick reads beside the units: the app's time, the looks, the sim's tick, which an
/// attack's phase reads, and the book of weapons, none in a match of no combat.
type PickReads<'w> = (
    Res<'w, Time>,
    Res<'w, UnitLooks>,
    Res<'w, SimTick>,
    Option<Res<'w, ById<ActionId, Weapon>>>,
);

/// The players of the models' clips, each with its graph.
type Players<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut AnimationPlayer,
        Option<&'static AnimationGraphHandle>,
    ),
>;

/// A drawing's root as its clips play: the root, its unit, its glide, which gives its walk, and
/// its states.
type PlayedRoot<'a> = (Entity, &'a DrawingOf, &'a Glide, &'a mut LookStates);

/// What a draw's models hold that its clip and its rise pose: their dressing and the meshes their
/// clip hides, their clips' players, their nodes' visibilities, and their places.
type ModelParts<'w, 's> = (
    Query<'w, 's, (&'static Dressed, &'static mut ClipHidden)>,
    Players<'w, 's>,
    Query<'w, 's, &'static mut Visibility>,
    Query<'w, 's, &'static mut Transform>,
);

/// The loaded scenes and clips, the clips' timings and graphs, and the asset server.
type ShowAssets<'w> = (
    ResMut<'w, ModelScenes>,
    ResMut<'w, LookClips>,
    Res<'w, Assets<ClipTiming>>,
    ResMut<'w, Assets<AnimationGraph>>,
    Res<'w, AssetServer>,
);

impl Plugin for LookStatesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LookClips>();
        app.init_asset::<ClipTiming>();
        app.add_systems(
            Update,
            (Self::pick, Self::play)
                .chain()
                .after(DrawSystems)
                .in_set(ViewSystems),
        );
    }
}

impl LookStates {
    /// The barrels the weapon slot `slot` fires from, of the first of its draws whose shown state,
    /// or the state its transition goes to, names them, of its look in `looks`; none when none
    /// does.
    pub(crate) fn barrels<'l>(&self, looks: &'l UnitLooks, slot: u8) -> Option<&'l [FireBarrel]> {
        let draws = looks.draws(self.unit_type, self.variation)?.draws();
        self.draws.iter().zip(draws).find_map(|(show, draw)| {
            let state = &draw.states()[show.state()?];
            let bones = state.fire_bones.iter().find(|bones| bones.slot == slot)?;
            Some(&*bones.barrels)
        })
    }

    /// The states of a unit of `unit_type` and `variation` that draws `draws`, none picked yet,
    /// its dice of `seed`.
    pub(crate) fn new(
        unit_type: UnitType,
        variation: Option<Variation>,
        draws: &LookDraws,
        seed: u64,
    ) -> Self {
        Self {
            unit_type,
            variation,
            held: None,
            lacks: Vec::new(),
            moved: Duration::ZERO,
            shots: SlotShots::default(),
            dice: LookDice(seed),
            draws: vec![DrawShow::default(); draws.draws().len()],
        }
    }
}

impl LookDice {
    /// The next of its numbers, SplitMix64's.
    const fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `count`, which is above 0.
    pub(crate) fn below(&mut self, count: usize) -> usize {
        debug_assert!(count > 0, "a number below none");
        let count = u64::try_from(count).expect("a usize fits u64");
        usize::try_from(self.next() % count).expect("below a usize")
    }

    /// A number from `least` to `most`.
    pub(crate) fn between(&mut self, least: f32, most: f32) -> f32 {
        let unit = (self.next() >> 40) as f32 / (1_u64 << 24) as f32;
        least + (most - least) * unit
    }
}

impl LookStatesPlugin {
    /// Picks the states of each new drawing, of each unit whose conditions' parts changed, that
    /// moved, or that came back from its death, and of each that held a condition time ends, of its walk or its attack, as it
    /// may hold another now; and asks each draw to show its state anew.
    fn pick(
        (time, looks, tick, weapons): PickReads<'_>,
        (mut scenes, mut clips, timings, mut graphs, assets): ShowAssets<'_>,
        (retested, moved, mut revived, drawn): (
            Retested<'_, '_>,
            Moved<'_, '_>,
            RemovedComponents<'_, '_, Dead>,
            Query<'_, '_, &'static Drawing, Allow<Unpredicted>>,
        ),
        (units, armed): (UnitParts<'_, '_>, AttackQuery<'_, '_>),
        (replacing, mut roots): (Query<'_, '_, &'static Replaces>, Roots<'_, '_>),
        (mut timed, mut visits, mut lacks): PickScratch<'_>,
        mut commands: Commands<'_, '_>,
    ) {
        let now = time.elapsed();
        visits.clear();
        visits.extend(roots.p0().iter());
        visits.extend(retested.iter().map(Drawing::root));
        let revived = revived.read().filter_map(|unit| drawn.get(unit).ok());
        visits.extend(revived.map(Drawing::root));
        let mut roots = roots.p1();
        for drawing in &moved {
            if let Ok((_, mut states)) = roots.get_mut(drawing.root()) {
                states.moved = now;
                visits.push(drawing.root());
            }
        }
        visits.append(&mut timed);
        visits.sort_unstable();
        visits.dedup();
        for &root in &*visits {
            let Ok((&DrawingOf(unit), mut states)) = roots.get_mut(root) else {
                continue;
            };
            let (Ok((life, &tags, destination, site, dead, &team)), Some(draws), Some(tests)) = (
                units.get(unit),
                looks.draws(states.unit_type, states.variation),
                looks.tests(states.unit_type),
            ) else {
                continue;
            };
            let groups = looks.weapons(states.unit_type);
            let attack = armed.get(unit).ok().zip(weapons.as_deref());
            lacks.clear();
            let attacks = match attack {
                Some(((progress, slots, times, clips), weapons)) if !groups.is_empty() => {
                    let at = tick.start();
                    if let Some(times) = times {
                        states.shots.see(times.iter().map(|time| time.ready_at), at);
                    }
                    let parts = AttackParts {
                        progress,
                        slots,
                        times,
                        clips,
                        weapons,
                        now: at,
                    };
                    Self::lacks(&parts, &states.shots, groups, &mut lacks);
                    Self::attacks(&parts, &states.shots, groups)
                }
                _ => 0,
            };
            // A clip that reloads comes back full with no change the client sees.
            if lacks.iter().any(|lack| lack.reloads) {
                timed.push(root);
            }
            let rounds = lacks.iter().map(|lack| lack.rounds);
            let lacked = if states.lacks.iter().copied().eq(rounds.clone()) {
                Relacked::Same
            } else {
                states.lacks.clear();
                states.lacks.extend(rounds);
                Relacked::Changed
            };
            let facts = UnitFacts {
                tags: tags.tags,
                walks: destination.is_some_and(|destination| destination.get().is_some()),
                still_for: now.saturating_sub(states.moved),
                life: life.map(|life| [life.current(), life.max()]),
                site: site.map(|(_, progress)| {
                    if progress.get() == Num::ZERO {
                        SiteStage::Waiting
                    } else {
                        SiteStage::Started
                    }
                }),
                dead,
                attacks,
            };
            let held = tests.held(facts);
            if held & tests.timed() != 0 {
                timed.push(root);
            }
            let picked = states.held != Some(held);
            if !picked && lacked == Relacked::Same {
                continue;
            }
            let changed = states.held.map_or(0, |before| before ^ held);
            states.held = Some(held);
            let LookStates {
                dice,
                draws: shows,
                lacks: lacked_rounds,
                ..
            } = &mut *states;
            let mut cx = ShowCx {
                root,
                paint: Paint::of(team),
                held,
                lacks: lacked_rounds,
                dice,
                scenes: &mut scenes,
                clips: &mut clips,
                timings: &timings,
                graphs: &mut graphs,
                assets: &assets,
                replacing: &replacing,
                commands: &mut commands,
            };
            for (draw, show) in draws.draws().iter().zip(shows) {
                if picked {
                    show.request(draw, draw.best(held), &mut cx);
                }
                show.renode(draw, changed, lacked, &mut cx);
            }
        }
    }

    /// What the clip of each of its look's weapon groups `groups` lacks, by the groups' places, of a
    /// unit of `parts`, its shots `shots` seen, into `lacks`: the clip of its first slot of a
    /// weapon of the group, none for a group of no such slot.
    fn lacks(
        parts: &AttackParts<'_>,
        shots: &SlotShots,
        groups: &[Box<[ActionId]>],
        lacks: &mut Vec<ClipLack>,
    ) {
        lacks.extend(groups.iter().map(|weapons| {
            let slots = (0..=u8::MAX).zip(parts.slots.iter());
            let mut held = slots
                .filter(|(_, held)| held.action.is_some_and(|action| weapons.contains(&action)));
            held.next().map_or_else(ClipLack::default, |(slot, _)| {
                parts.slot(shots, slot).lack()
            })
        }));
    }

    /// The bits of `UnitFacts::attacks` of a unit of `parts`, its shots `shots` seen: each phase
    /// a slot of its look's weapon group of `groups` is at.
    fn attacks(parts: &AttackParts<'_>, shots: &SlotShots, groups: &[Box<[ActionId]>]) -> u64 {
        let mut attacks = 0;
        for (slot, held) in (0..=u8::MAX).zip(parts.slots.iter()) {
            let Some(action) = held.action else {
                continue;
            };
            let Some(phase) = parts.slot(shots, slot).phase() else {
                continue;
            };
            for (group, weapons) in (0..=u8::MAX).zip(groups) {
                if weapons.contains(&action)
                    && let Some(bit) = UnitFacts::attack_bit(group, phase)
                {
                    attacks |= bit;
                }
            }
        }
        attacks
    }

    /// Moves the clip of each draw of each drawing by the frame's time, its drawing's walk setting
    /// the rate of a clip of a stride, and poses its models by it: each model's player plays the
    /// clip's graph, held at the clip's time, and each pivot's own meshes the clip hides at its
    /// frame are hidden, but those its look hides. A state that rises sinks its models by its
    /// rise times the share of its unit's site not yet built, as Zero Hour's
    /// `ADJUST_HEIGHT_BY_CONSTRUCTION_PERCENT` does, and any other draws them at their place
    /// (04-capabilities/production.md, A building rises with its site).
    fn play(
        (time, fixed, looks): (Res<'_, Time>, Res<'_, Time<Fixed>>, Res<'_, UnitLooks>),
        (mut scenes, mut clips, timings, mut graphs, assets): ShowAssets<'_>,
        units: UnitParts<'_, '_>,
        (replacing, mut roots): (
            Query<'_, '_, &'static Replaces>,
            Query<'_, '_, PlayedRoot<'static>>,
        ),
        (mut dressed, mut players, mut visibilities, mut transforms): ModelParts<'_, '_>,
        mut hidden: Local<'_, Vec<Entity>>,
        mut commands: Commands<'_, '_>,
    ) {
        let seconds = time.delta_secs();
        let clock = FrameClock {
            timestep: fixed.timestep(),
            fraction: fixed.overstep_fraction(),
        };
        for (root, &DrawingOf(unit), glide, mut states) in &mut roots {
            let Ok((_, _, _, site, _, &team)) = units.get(unit) else {
                continue;
            };
            let unbuilt = site.map_or(0.0, |(site, progress)| {
                (Num::ONE - progress.share(site)).float()
            });
            let Some(draws) = looks.draws(states.unit_type, states.variation) else {
                continue;
            };
            let speed = glide.speed(time.elapsed(), clock);
            let held = states.held.unwrap_or(0);
            let LookStates {
                dice,
                draws: shows,
                lacks,
                ..
            } = &mut *states;
            let mut cx = ShowCx {
                root,
                paint: Paint::of(team),
                held,
                lacks,
                dice,
                scenes: &mut scenes,
                clips: &mut clips,
                timings: &timings,
                graphs: &mut graphs,
                assets: &assets,
                replacing: &replacing,
                commands: &mut commands,
            };
            for (draw, show) in draws.draws().iter().zip(shows.iter_mut()) {
                show.advance(draw, ClipStep { seconds, speed }, &mut cx);
                let sunk = -show.rise(draw).map_or(0.0, |rise| rise * unbuilt);
                for &model in show.models() {
                    if let Ok(mut transform) = transforms.get_mut(model)
                        && transform.translation.y != sunk
                    {
                        transform.translation.y = sunk;
                    }
                }
                let Some(pose) = show.pose(&timings) else {
                    continue;
                };
                for &model in show.models() {
                    let Ok((dressed, mut clip_hidden)) = dressed.get_mut(model) else {
                        continue;
                    };
                    let Some(pivots) = &dressed.pivots else {
                        continue;
                    };
                    if let Ok((mut playing, graph)) = players.get_mut(pivots.player) {
                        if graph.is_none_or(|graph| graph.0 != pose.clip.graph) {
                            let graph = AnimationGraphHandle(pose.clip.graph.clone());
                            cx.commands.entity(pivots.player).insert(graph);
                            playing.stop_all();
                        }
                        let node = pose.clip.node;
                        if !playing.is_playing_animation(node) {
                            playing.play(node).pause();
                        }
                        if let Some(active) = playing.animation_mut(node) {
                            active.seek_to(pose.seconds);
                        }
                    }
                    let mut hiding = ClipHiding {
                        scratch: &mut hidden,
                        visibilities: &mut visibilities,
                    };
                    hiding.hide(dressed, &mut clip_hidden, pose);
                }
            }
        }
    }
}

/// What hides the meshes a clip hides: a scratch list of them, and their visibilities.
#[derive(Debug)]
struct ClipHiding<'a, 'w, 's> {
    scratch: &'a mut Vec<Entity>,
    visibilities: &'a mut Query<'w, 's, &'static mut Visibility>,
}

impl ClipHiding<'_, '_, '_> {
    /// Hides the mesh nodes of `dressed`'s pivots that the clip of `pose` hides at its frame, but
    /// those its look hides, and shows again those it hid that it no longer hides, the clip's
    /// pivots resolved by their names into `hidden` as the model first plays it; none for a model
    /// of no pivots (02-engine-core.md, A visibility channel hides its pivot's own meshes).
    fn hide(&mut self, dressed: &Dressed, hidden: &mut ClipHidden, pose: ClipPose<'_>) {
        let Some(pivots) = &dressed.pivots else {
            return;
        };
        let timing = pose.clip.timing.id();
        if hidden.timing != Some(timing) {
            hidden.timing = Some(timing);
            hidden.pivots.clear();
            for (at, clip) in pose.timing.hidden.iter().enumerate() {
                let own = pivots.meshes.iter().enumerate();
                let named = own.filter(|(_, own)| own.pivot == clip.pivot);
                hidden
                    .pivots
                    .extend(named.map(|(pivot, _)| ClipPivot { hidden: at, pivot }));
            }
        }
        self.scratch.clear();
        for &ClipPivot { hidden: at, pivot } in &hidden.pivots {
            if pose.timing.hidden[at].hides(pose.frame) {
                let meshes = pivots.meshes[pivot].meshes.iter();
                self.scratch
                    .extend(meshes.filter(|mesh| !dressed.hidden.contains(mesh)));
            }
        }
        if *self.scratch == hidden.meshes {
            return;
        }
        for &node in &hidden.meshes {
            if !self.scratch.contains(&node)
                && let Ok(mut visibility) = self.visibilities.get_mut(node)
            {
                visibility.set_if_neq(Visibility::Inherited);
            }
        }
        for &node in &*self.scratch {
            if let Ok(mut visibility) = self.visibilities.get_mut(node) {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
        hidden.meshes.clone_from(self.scratch);
    }
}

#[cfg(test)]
mod tests;
