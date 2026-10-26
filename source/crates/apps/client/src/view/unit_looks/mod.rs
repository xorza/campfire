use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use bevy::asset::{AssetPath, AssetServer};
use bevy::ecs::bundle::Bundle;
use bevy::ecs::component::Component;
use bevy::ecs::resource::Resource;
use bevy::gltf::GltfAssetLabel;
use bevy::math::Vec3;
use bevy::transform::components::Transform;
use bevy::world_serialization::WorldAssetRoot;
use campfire_client_files::{
    AnimationMode, AttackPhase, ClientModel, ConditionTest, FireBone, LifeShare, LookDraw,
    LookShown, ModelTop, SetMatch, SiteStage, StartFrame, StateAnimation,
};
use campfire_common::DeclaredName;
use campfire_declarations::ById;
use campfire_math::Num;
use campfire_script::MatchNames;
use campfire_units::{TypeOriginBook, Variation, Variations};
use campfire_values::{ActionId, NameId, PackageIndex, Tag, TagSet, UnitType};
use tracing::error;

use crate::view::client_data::ClientLoad;
use crate::view::float_num::FloatNum;
use crate::view::match_colors::Paint;
use crate::view::model_scenes::ModelScenes;
use crate::view::package_source::PackageAssets;

/// Each unit type's look, as its package's client data gives it, by type: none for a type of no
/// look, drawn as the engine's own shapes are; and what each package's conditions test of a
/// unit, by the package's place.
#[derive(Resource, Debug, Default)]
pub(crate) struct UnitLooks {
    looks: Vec<Option<TypeLook>>,
    tests: Vec<UnitTests>,
}

/// The match's unit types, whose looks load: the origin of each, its variations, none in a match
/// that holds no book of them, and the match's names, of which a condition's tag is one.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LookTypes<'a> {
    pub(crate) origins: &'a TypeOriginBook,
    pub(crate) variations: Option<&'a ById<UnitType, Variations>>,
    pub(crate) names: &'a MatchNames,
}

/// A type's look: its package, its own draws, those of each of its variations, none for a look
/// that does not vary, the angle in degrees a build preview places it at, and the groups of its
/// type's weapons its attack conditions test.
#[derive(Debug)]
struct TypeLook {
    package: PackageIndex,
    own: LookDraws,
    variations: Box<[LookDraws]>,
    placement_angle: Num,
    weapons: Box<[Box<[ActionId]>]>,
}

/// The draws a unit of a look draws, each of its states: none for a look drawn as nothing; the
/// models of each draw's rest state, the one of no condition the unit itself holds, which a build
/// preview and a projectile draw; and the top in meters of the highest of those, none when the
/// file says no model's.
#[derive(Debug)]
pub(crate) struct LookDraws {
    draws: Box<[DrawStates]>,
    rest: Vec<LookModel>,
    top: Option<f32>,
}

/// A draw's states, each its sets of conditions against its package's tests and the map; the one
/// it draws for a unit of no condition of its own; and its transitions.
#[derive(Debug)]
pub(crate) struct DrawStates {
    states: Box<[StateModels]>,
    rest: usize,
    transitions: Box<[TransitionModels]>,
}

/// A state of a draw: its sets of conditions, what it shows, its key, which names it to a
/// transition, the key of the state it lets finish before it is shown, and the barrels each
/// weapon slot fires from.
#[derive(Debug)]
pub(crate) struct StateModels {
    sets: Box<[SetMask]>,
    pub(crate) shown: Shown,
    pub(crate) key: Option<StateKey>,
    pub(crate) waits_for: Option<StateKey>,
    pub(crate) fire_bones: Box<[FireBone]>,
}

/// A key a draw's states and transitions name, by its place among the draw's keys, resolved as
/// the look loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StateKey(usize);

/// What a draw shows between a state of the key `from` and one of the key `to`.
#[derive(Debug)]
pub(crate) struct TransitionModels {
    from: StateKey,
    to: StateKey,
    pub(crate) shown: Shown,
}

/// What a state or a transition shows: its models, its animation, and the meters its models rise
/// by as its unit's site is built.
#[derive(Debug)]
pub(crate) struct Shown {
    pub(crate) models: Vec<LookModel>,
    pub(crate) animation: Option<StateClips>,
    pub(crate) rise: Option<f32>,
}

/// What a draw shows of its states and transitions: a state, or a transition, by its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Showing {
    State(usize),
    Transition(usize),
}

/// How a state plays its animation, as its file says it, its clips by their files' paths
/// (02-engine-core.md, A state plays its animation by its mode).
#[derive(Debug)]
pub(crate) struct StateClips {
    pub(crate) clips: Vec<ClipFile>,
    pub(crate) idle: bool,
    pub(crate) mode: AnimationMode,
    pub(crate) start: StartFrame,
    pub(crate) maintain: u8,
    pub(crate) restart: bool,
    pub(crate) speed: [f32; 2],
}

/// A clip's animation file, and the meters its unit walks in one run of it, none for one played
/// at its own rate whatever its unit's speed.
#[derive(Debug)]
pub(crate) struct ClipFile {
    pub(crate) path: AssetPath<'static>,
    pub(crate) stride: Option<f32>,
}

/// A set of conditions as a unit's are matched against it: the tested conditions it holds, by
/// their bits, and how many of the others a unit always holds, as the map's, and never holds, as
/// a condition of no test, a map's condition the map lacks, or one its draw ignores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SetMask {
    tested: u64,
    held: u32,
    lacked: u32,
}

/// What a package's conditions test of a unit, each by its bit: the tests that tell one unit from
/// another, the map's held for the whole match apart.
#[derive(Debug, Default)]
pub(crate) struct UnitTests {
    tests: Vec<UnitTest>,
}

/// What a tested condition reads of a unit (09-zero-hour.md, A condition is the game's name for
/// a test).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnitTest {
    Tag(Tag),
    /// Its unit walks and moved within this long.
    Moving(Duration),
    Life(LifeShare),
    Site(SiteStage),
    Dead,
    /// A weapon of its look's group at a place is at a phase, or at any, by their bits in
    /// `UnitFacts::attacks`.
    Attack(u64),
}

/// A package's tests, and what each of its conditions is to a look.
#[derive(Debug)]
struct PackageTests {
    tests: UnitTests,
    known: BTreeMap<DeclaredName, Known>,
}

/// What a package's condition is to a look: a tested one, by its bit, or one every unit holds,
/// or one none holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Known {
    Tested(u32),
    Held,
    Lacked,
}

/// What a unit holds that its look's tests read: its tags, whether it walks to a destination, how
/// long since it last moved, its life and its most life, none for a unit of no life, its site's
/// stage, none for a unit that is no site, whether it is dead, and the phases of its look's weapon groups, by their
/// bits, as `attack_bit` places them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnitFacts {
    pub(crate) tags: TagSet,
    pub(crate) walks: bool,
    pub(crate) still_for: Duration,
    pub(crate) life: Option<[Num; 2]>,
    pub(crate) site: Option<SiteStage>,
    pub(crate) dead: bool,
    pub(crate) attacks: u64,
}

/// A model a look is drawn with: the path of its glTF's first scene, its nodes, and the factor it
/// is drawn at.
#[derive(Debug, Clone)]
pub(crate) struct LookModel {
    pub(crate) scene: AssetPath<'static>,
    pub(crate) nodes: LookNodes,
    pub(crate) scale: f32,
    upgrades: Arc<[ModelUpgrade]>,
}

/// The nodes of a look's model a condition shows and hides while its unit holds it, over those
/// its state hides (09-zero-hour.md, An upgrade's look is a tag).
#[derive(Debug, Clone)]
struct ModelUpgrade {
    when: Known,
    show: Box<[String]>,
    hide: Box<[String]>,
}

/// Where a look's models come from: their package, the node they turn by their unit's turret
/// angle, and their package's conditions, which their upgrades name, none for models no unit
/// draws.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ModelSource<'a> {
    package: PackageIndex,
    turret: Option<&'a str>,
    known: Option<&'a BTreeMap<DeclaredName, Known>>,
}

/// What a drawn model needs once its scene is spawned: the names of the nodes its look hides, and
/// the name of the node its look turns by its unit's turret angle, if it has one.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct LookNodes {
    pub(crate) hide: Arc<[String]>,
    pub(crate) turret: Option<Arc<str>>,
}

impl UnitLooks {
    /// The most tests a package's conditions hold, one for each bit a unit's conditions hold.
    const MOST_TESTS: usize = 64;

    /// The looks of every type of `types`: the draws of the type's key in its package's units,
    /// and of each of its look's variations, each state's sets against its package's tests and
    /// the map's conditions; a look whose variations are not as many as its type's is logged,
    /// and each of its units draws its own (02-engine-core.md, A look varies by its unit's
    /// variation).
    pub(crate) fn of(types: LookTypes<'_>, data: &ClientLoad) -> Self {
        let mut known = Vec::with_capacity(data.packages.len());
        let mut tests = Vec::with_capacity(data.packages.len());
        for package in &data.packages {
            let PackageTests {
                tests: package_tests,
                known: package_known,
            } = UnitTests::of(
                package.units.conditions(),
                &data.map_conditions,
                types.names,
            );
            tests.push(package_tests);
            known.push(package_known);
        }
        let looks = types
            .origins
            .iter()
            .map(|(unit_type, origin)| {
                let package = origin.package;
                let unit = data.of(package).units.units().get(&*origin.name)?;
                let known = &known[usize::from(package.get())];
                let load = |draws: &[LookDraw], turret: Option<&str>| {
                    LookDraws::load(package, draws, turret, known)
                };
                let of_type = types.variations.and_then(|book| book.get(unit_type));
                let count = of_type.map_or(0, Variations::len);
                let fits = unit.variations.is_empty() || unit.variations.len() == count;
                if !fits {
                    let (look, looks) = (&*origin.name, unit.variations.len());
                    error!(look, looks, count, "a look's variations are not its type's");
                }
                let variations = unit.variations.iter().filter(|_| fits);
                Some(TypeLook {
                    package,
                    own: load(&unit.draws, unit.turret.as_deref()),
                    variations: variations
                        .map(|variation| load(&variation.draws, variation.turret.as_deref()))
                        .collect(),
                    placement_angle: unit.placement_angle.unwrap_or(Num::ZERO),
                    weapons: unit
                        .weapons
                        .iter()
                        .map(|group| {
                            let named = group.iter().filter_map(|weapon| {
                                let found =
                                    types.names.named::<ActionId>((package, weapon.as_str()));
                                if found.is_none() {
                                    let (look, weapon) = (&*origin.name, weapon.as_str());
                                    error!(look, weapon, "a look's weapon names no action");
                                }
                                found
                            });
                            named.collect()
                        })
                        .collect(),
                })
            })
            .collect();
        Self { looks, tests }
    }

    /// The draws a unit of `unit_type` and `variation` draws: its variation's, or its look's own
    /// for a unit of none, of a look that does not vary, or of a place past the look's; `None`
    /// for a type of no look in the client data.
    pub(crate) fn draws(
        &self,
        unit_type: UnitType,
        variation: Option<Variation>,
    ) -> Option<&LookDraws> {
        let look = self.looks.get(unit_type.index())?.as_ref()?;
        let place = variation.map(|variation| usize::from(variation.get()));
        Some(
            place
                .and_then(|place| look.variations.get(place))
                .unwrap_or(&look.own),
        )
    }

    /// The models a unit of `unit_type` and `variation` draws at rest, as a build preview and a
    /// projectile draw them; `None` for a type of no look in the client data.
    pub(crate) fn look(
        &self,
        unit_type: UnitType,
        variation: Option<Variation>,
    ) -> Option<&[LookModel]> {
        Some(&self.draws(unit_type, variation)?.rest)
    }

    /// The models each look draws at rest, of each of its types and variations, which load as
    /// the match starts.
    pub(crate) fn rest_models(&self) -> impl Iterator<Item = &LookModel> {
        let looks = self.looks.iter().flatten();
        let draws = looks.flat_map(|look| std::iter::once(&look.own).chain(&*look.variations));
        draws.flat_map(|draws| &draws.rest)
    }

    /// What the conditions of `unit_type`'s package test of a unit; none for a type of no look.
    pub(crate) fn tests(&self, unit_type: UnitType) -> Option<&UnitTests> {
        let look = self.looks.get(unit_type.index())?.as_ref()?;
        self.tests.get(usize::from(look.package.get()))
    }

    /// The groups of `unit_type`'s weapons its look's attack conditions test, each by its place;
    /// none for a type of no look.
    pub(crate) fn weapons(&self, unit_type: UnitType) -> &[Box<[ActionId]>] {
        let look = self.looks.get(unit_type.index()).and_then(Option::as_ref);
        look.map_or(&[], |look| &look.weapons)
    }

    /// The angle in degrees a build preview places `unit_type` at: its look's, 0 for a look that
    /// gives none or a type of no look.
    pub(crate) fn placement_angle(&self, unit_type: UnitType) -> Num {
        let look = self.looks.get(unit_type.index()).and_then(Option::as_ref);
        look.map_or(Num::ZERO, |look| look.placement_angle)
    }
}

impl LookDraws {
    /// The draws of `draws`, a look of the package at `package` whose models turn `turret` by
    /// their unit's turret angle, each set against the package's conditions `known`.
    fn load(
        package: PackageIndex,
        draws: &[LookDraw],
        turret: Option<&str>,
        known: &BTreeMap<DeclaredName, Known>,
    ) -> Self {
        let mut top: Option<ModelTop> = None;
        let mut loaded = Vec::with_capacity(draws.len());
        for draw in draws {
            let states = DrawStates::load(package, draw, turret, known);
            if let Some(rest) = draw.states()[states.rest].top()
                && top.is_none_or(|top| rest.get() > top.get())
            {
                top = Some(rest);
            }
            loaded.push(states);
        }
        let rest = loaded
            .iter()
            .flat_map(|draw| draw.states[draw.rest].shown.models.iter().cloned())
            .collect();
        Self {
            draws: loaded.into_boxed_slice(),
            rest,
            top: top.map(ModelTop::get),
        }
    }

    pub(crate) fn draws(&self) -> &[DrawStates] {
        &self.draws
    }

    /// The top in meters of the highest model it draws at rest; none when the file says no
    /// model's.
    pub(crate) const fn top(&self) -> Option<f32> {
        self.top
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.draws.is_empty()
    }
}

impl DrawStates {
    fn load(
        package: PackageIndex,
        draw: &LookDraw,
        turret: Option<&str>,
        known: &BTreeMap<DeclaredName, Known>,
    ) -> Self {
        let ignored = |name: &DeclaredName| draw.ignore().contains(name);
        let shown =
            |models: &[ClientModel], animation: Option<&StateAnimation>, rise: Option<Num>| Shown {
                models: LookModel::all(
                    ModelSource {
                        package,
                        turret,
                        known: Some(known),
                    },
                    models,
                ),
                animation: animation.map(|animation| StateClips::of(package, animation)),
                rise: rise.map(FloatNum::float),
            };
        let mut keys: Vec<DeclaredName> = Vec::new();
        let mut key = |name: &DeclaredName| {
            let at = keys
                .iter()
                .position(|known| known == name)
                .unwrap_or_else(|| {
                    keys.push(name.clone());
                    keys.len() - 1
                });
            StateKey(at)
        };
        let states: Box<[StateModels]> = draw
            .states()
            .iter()
            .map(|state| StateModels {
                sets: state
                    .sets
                    .iter()
                    .map(|set| SetMask::of(set, known, ignored))
                    .collect(),
                shown: shown(&state.models, state.animation.as_ref(), state.rise),
                key: state.key.as_ref().map(&mut key),
                waits_for: state.waits_for.as_ref().map(&mut key),
                fire_bones: state.fire_bones.clone().into_boxed_slice(),
            })
            .collect();
        let transitions = draw
            .transitions()
            .iter()
            .map(|transition| TransitionModels {
                from: key(transition.from()),
                to: key(transition.to()),
                shown: {
                    let LookShown {
                        models,
                        animation,
                        rise,
                    } = transition.shown();
                    shown(models, animation.as_ref(), *rise)
                },
            })
            .collect();
        let rest = Self::best_of(&states, 0);
        Self {
            states,
            rest,
            transitions,
        }
    }

    pub(crate) fn states(&self) -> &[StateModels] {
        &self.states
    }

    /// What it shows at `showing`.
    pub(crate) fn shown(&self, showing: Showing) -> &Shown {
        match showing {
            Showing::State(at) => &self.states[at].shown,
            Showing::Transition(at) => &self.transitions[at].shown,
        }
    }

    /// The transition it shows from a state of the key `from` to one of the key `to`, as
    /// `findTransitionForSig` finds it; none for a pair of no transition.
    pub(crate) fn transition(&self, from: StateKey, to: StateKey) -> Option<usize> {
        self.transitions
            .iter()
            .position(|transition| transition.from == from && transition.to == to)
    }

    /// The state a unit that holds the tested conditions `held` draws, as `SetMatch::best` finds
    /// it.
    pub(crate) fn best(&self, held: u64) -> usize {
        Self::best_of(&self.states, held)
    }

    fn best_of(states: &[StateModels], held: u64) -> usize {
        SetMatch::best(states.iter().map(|state| {
            state.sets.iter().map(|set| SetMatch {
                common: (set.tested & held).count_ones() + set.held,
                lacked: (set.tested & !held).count_ones() + set.lacked,
            })
        }))
    }
}

impl StateClips {
    /// The clips of `animation`, a state's of the package at `package`.
    fn of(package: PackageIndex, animation: &StateAnimation) -> Self {
        let speed = animation.speed();
        Self {
            clips: animation
                .clips()
                .iter()
                .map(|clip| ClipFile {
                    path: AssetPath::from(PackageAssets::asset_path(package, &clip.animation)),
                    stride: clip.stride.map(FloatNum::float),
                })
                .collect(),
            idle: animation.idle(),
            mode: animation.mode(),
            start: animation.start(),
            maintain: animation.maintain(),
            restart: animation.restart(),
            speed: [speed.least(), speed.most()].map(FloatNum::float),
        }
    }
}

impl SetMask {
    /// The mask of `set`, of conditions `known` by their package, those `ignored` never held.
    fn of(
        set: &[DeclaredName],
        known: &BTreeMap<DeclaredName, Known>,
        ignored: impl Fn(&DeclaredName) -> bool,
    ) -> Self {
        let mut mask = Self {
            tested: 0,
            held: 0,
            lacked: 0,
        };
        for name in set {
            let known = *known
                .get(name)
                .expect("a read units file names no condition of no test");
            match known {
                _ if ignored(name) => mask.lacked += 1,
                Known::Tested(bit) => mask.tested |= 1 << bit,
                Known::Held => mask.held += 1,
                Known::Lacked => mask.lacked += 1,
            }
        }
        mask
    }
}

impl UnitTests {
    /// The tests of `conditions`, a package's, on a map of `map`, and what each condition is to a
    /// look: a map's condition held or lacked for the whole match, a condition of no test lacked,
    /// and each other tested, by its bit. A tag the match does not name, and a test past the
    /// `MOST_TESTS`, are logged, and lacked.
    fn of(
        conditions: &BTreeMap<DeclaredName, ConditionTest>,
        map: &[DeclaredName],
        names: &MatchNames,
    ) -> PackageTests {
        let mut tests = Self::default();
        let mut known = BTreeMap::new();
        for (name, test) in conditions {
            let tested = match test {
                ConditionTest::Map => {
                    let held = map.contains(name);
                    known.insert(name.clone(), if held { Known::Held } else { Known::Lacked });
                    continue;
                }
                ConditionTest::Never => None,
                ConditionTest::Tag { tag } => {
                    let found = names.named::<Tag>(tag.as_str());
                    if found.is_none() {
                        let (condition, tag) = (name.as_str(), tag.as_str());
                        error!(condition, tag, "a look's condition names a tag of no name");
                    }
                    found.map(UnitTest::Tag)
                }
                ConditionTest::Moving { still_ms } => Some(UnitTest::Moving(
                    Duration::from_millis(u64::from(*still_ms)),
                )),
                ConditionTest::Life(share) => Some(UnitTest::Life(*share)),
                ConditionTest::Site { stage } => Some(UnitTest::Site(*stage)),
                ConditionTest::Dead => Some(UnitTest::Dead),
                ConditionTest::Attack { group, phase } => {
                    let bits = match phase {
                        Some(phase) => UnitFacts::attack_bit(*group, *phase),
                        None => UnitFacts::attack_bits(*group),
                    };
                    if bits.is_none() {
                        let (condition, group) = (name.as_str(), *group);
                        error!(
                            condition,
                            group, "a look's condition tests a weapon group past the last"
                        );
                    }
                    bits.map(UnitTest::Attack)
                }
            };
            let bit = tested.and_then(|test| {
                if tests.tests.len() >= UnitLooks::MOST_TESTS {
                    error!(
                        condition = name.as_str(),
                        "a package's looks test too many conditions"
                    );
                    return None;
                }
                tests.tests.push(test);
                Some(u32::try_from(tests.tests.len() - 1).expect("64 tests fit u32"))
            });
            known.insert(name.clone(), bit.map_or(Known::Lacked, Known::Tested));
        }
        PackageTests { tests, known }
    }

    /// The tested conditions a unit of `facts` holds, by their bits.
    pub(crate) fn held(&self, facts: UnitFacts) -> u64 {
        let mut held = 0;
        for (bit, test) in self.tests.iter().enumerate() {
            let holds = match *test {
                UnitTest::Tag(tag) => facts.tags.contains(tag),
                UnitTest::Moving(still) => facts.walks && facts.still_for <= still,
                UnitTest::Life(share) => facts
                    .life
                    .is_some_and(|[current, max]| share.holds(current, max)),
                UnitTest::Site(stage) => facts.site == Some(stage),
                UnitTest::Dead => facts.dead,
                UnitTest::Attack(bit) => facts.attacks & bit != 0,
            };
            if holds {
                held |= 1 << bit;
            }
        }
        held
    }

    /// The bits of its tests that time ends while nothing else of the unit changes: a walk's, as
    /// it stands still, and an attack's, as each phase ends.
    pub(crate) fn timed(&self) -> u64 {
        let mut timed = 0;
        for (bit, test) in self.tests.iter().enumerate() {
            if matches!(test, UnitTest::Moving(_) | UnitTest::Attack(_)) {
                timed |= 1 << bit;
            }
        }
        timed
    }
}

impl Known {
    /// Whether a unit that holds the tested conditions `held` holds it.
    const fn holds(self, held: u64) -> bool {
        match self {
            Self::Tested(bit) => (held >> bit) & 1 == 1,
            Self::Held => true,
            Self::Lacked => false,
        }
    }
}

impl ModelSource<'_> {
    /// The source of models of the package at `package` no unit draws, of no turret, whose
    /// upgrades none holds.
    pub(crate) const fn still(package: PackageIndex) -> Self {
        Self {
            package,
            turret: None,
            known: None,
        }
    }
}

impl UnitFacts {
    /// The bit of `attacks` of the weapon group at `group` at `phase`, four for each group; none
    /// for a group past the sixteen it holds.
    pub(crate) fn attack_bit(group: u8, phase: AttackPhase) -> Option<u64> {
        let phase = match phase {
            AttackPhase::PreAttack => 0,
            AttackPhase::Firing => 1,
            AttackPhase::Between => 2,
            AttackPhase::Reloading => 3,
        };
        let first = Self::attack_bits(group)?.trailing_zeros();
        Some(1 << (first + phase))
    }

    /// The bits of `attacks` of the weapon group at `group`, at any of its four phases; none for a
    /// group past the sixteen it holds.
    pub(crate) fn attack_bits(group: u8) -> Option<u64> {
        let at = u32::from(group) * 4;
        (at < u64::BITS).then(|| 0b1111 << at)
    }
}

impl LookModel {
    /// What a drawing's child of this model holds: its scene of `scenes`, which loads through
    /// `assets` the first time it is drawn, its nodes, its scale and `paint`.
    pub(crate) fn bundle(
        &self,
        scenes: &mut ModelScenes,
        assets: &AssetServer,
        paint: Paint,
    ) -> impl Bundle {
        (
            WorldAssetRoot(scenes.scene(&self.scene, assets)),
            self.nodes.clone(),
            Transform::from_scale(Vec3::splat(self.scale)),
            paint,
        )
    }

    /// Its nodes for a unit that holds the tested conditions `held`: those its state hides, then
    /// shown and hidden by each upgrade whose condition it holds, in order, as Zero Hour's
    /// `updateSubObjects` sets an upgrade's sub-objects over a state's.
    pub(crate) fn held_nodes(&self, held: u64) -> LookNodes {
        let mut applied = self
            .upgrades
            .iter()
            .filter(|upgrade| upgrade.when.holds(held));
        let Some(first) = applied.next() else {
            return self.nodes.clone();
        };
        let mut hide = self.nodes.hide.to_vec();
        for upgrade in std::iter::once(first).chain(applied) {
            hide.retain(|node| !upgrade.show.contains(node));
            for node in &upgrade.hide {
                if !hide.contains(node) {
                    hide.push(node.clone());
                }
            }
        }
        LookNodes {
            hide: hide.into(),
            turret: self.nodes.turret.clone(),
        }
    }

    /// The bits of the tested conditions its upgrades show and hide its nodes by.
    pub(crate) fn upgraded(&self) -> u64 {
        let tested = self
            .upgrades
            .iter()
            .filter_map(|upgrade| match upgrade.when {
                Known::Tested(bit) => Some(1 << bit),
                Known::Held | Known::Lacked => None,
            });
        tested.fold(0, |bits, bit| bits | bit)
    }

    /// Whether `other` draws as it does but for its hidden nodes, its scene at its scale.
    pub(crate) fn same_scene(&self, other: &Self) -> bool {
        self.scene == other.scene && self.scale == other.scale
    }

    /// The models `models` of `source`.
    pub(crate) fn all(source: ModelSource<'_>, models: &[ClientModel]) -> Vec<Self> {
        let ModelSource {
            package,
            turret,
            known,
        } = source;
        models
            .iter()
            .map(|model| Self {
                upgrades: model
                    .upgrades
                    .iter()
                    .map(|upgrade| ModelUpgrade {
                        when: known
                            .and_then(|known| known.get(&upgrade.condition))
                            .copied()
                            .unwrap_or(Known::Lacked),
                        show: upgrade.show.as_slice().into(),
                        hide: upgrade.hide.as_slice().into(),
                    })
                    .collect(),
                scene: GltfAssetLabel::Scene(0)
                    .from_asset(PackageAssets::asset_path(package, &model.model)),
                nodes: LookNodes {
                    hide: model.hide.as_slice().into(),
                    turret: turret.map(Arc::from),
                },
                scale: model.scale.get(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
