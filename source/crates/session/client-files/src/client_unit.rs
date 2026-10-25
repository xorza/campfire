use campfire_common::DeclaredName;
use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::client_model::{ClientModel, ModelTop};
use crate::state_animation::{AnimationMode, StateAnimation};

/// A unit type's look: its draws, each of which draws its own state at its unit's place, all of
/// them; the look of each of its type's variations; the angle in degrees a client's preview of a
/// build places it at, 0 when it gives none; and the node its models turn by its unit's turret
/// angle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientUnit {
    pub draws: Vec<LookDraw>,
    /// By its type's variations' order, what a unit of each draws in place of the look's own;
    /// none for a look that does not vary, whose own a unit of any variation draws, as a build
    /// preview and a client object do (02-engine-core.md, A look varies by its unit's variation).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variations: Vec<LookVariation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement_angle: Option<Num>,
    /// The name of the node its models turn by the unit's turret angle
    /// (04-capabilities/combat.md, A unit type may have a turret).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turret: Option<String>,
}

/// The look of one of a type's variations: its draws, and the name of the node its models turn
/// by the unit's turret angle, as each variation's models name their own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookVariation {
    pub draws: Vec<LookDraw>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turret: Option<String>,
}

/// One of a look's draws, as a Zero Hour object draws each of its draw modules: its states, of
/// which a unit draws the one whose conditions best match its own, and the conditions it ignores
/// as it matches them, as no unit held them (animated-looks.md, A look is states of conditions).
/// The first state is the draw's default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LookDraw {
    states: Vec<LookState>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    ignore: Vec<DeclaredName>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    transitions: Vec<LookTransition>,
}

/// A state of a draw: the sets of condition names it is drawn for, each a set its unit may hold;
/// its models and its animation; its key, which names it to a transition; and the key of the
/// state it lets finish its animation before it is drawn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookState {
    pub sets: Vec<Vec<DeclaredName>>,
    pub models: Vec<ClientModel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<StateAnimation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<DeclaredName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waits_for: Option<DeclaredName>,
}

/// What a draw draws between a state of the key `from` and one of the key `to`: its models and
/// its animation, played once, as Zero Hour's `TransitionState` (animated-looks.md, A state plays
/// its animation by its mode).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LookTransition {
    from: DeclaredName,
    to: DeclaredName,
    models: Vec<ClientModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    animation: Option<StateAnimation>,
}

impl ClientUnit {
    /// The look of `draws`, of no variation.
    pub const fn new(draws: Vec<LookDraw>) -> Self {
        Self {
            draws,
            variations: Vec::new(),
            placement_angle: None,
            turret: None,
        }
    }

    /// A look of `models` alone, drawn whatever its unit holds: one draw of one state, drawn for
    /// no condition; no draw for no model.
    pub fn still(models: Vec<ClientModel>) -> Self {
        let draws = (!models.is_empty()).then(|| LookDraw::still(models));
        Self::new(draws.into_iter().collect())
    }

    /// Each draw of it and of each of its variations.
    pub fn every_draw(&self) -> impl Iterator<Item = &LookDraw> {
        let variations = self
            .variations
            .iter()
            .flat_map(|variation| &variation.draws);
        self.draws.iter().chain(variations)
    }
}

impl LookDraw {
    /// The draw of `states`, its default first, that ignores `ignore`, of `transitions`, each
    /// of a pair of keys once; none for no state, for a state of no set of conditions, which no
    /// unit draws, or for two transitions of one pair.
    pub fn new(
        states: Vec<LookState>,
        ignore: Vec<DeclaredName>,
        transitions: Vec<LookTransition>,
    ) -> Option<Self> {
        let drawn = !states.is_empty() && states.iter().all(|state| !state.sets.is_empty());
        let mut pairs: Vec<[&DeclaredName; 2]> = transitions
            .iter()
            .map(|transition| [&transition.from, &transition.to])
            .collect();
        pairs.sort_unstable();
        let once = pairs.windows(2).all(|pair| pair[0] != pair[1]);
        (drawn && once).then_some(Self {
            states,
            ignore,
            transitions,
        })
    }

    /// The draw of `models` alone, drawn for no condition.
    pub fn still(models: Vec<ClientModel>) -> Self {
        Self {
            states: vec![LookState::new(vec![Vec::new()], models)],
            ignore: Vec::new(),
            transitions: Vec::new(),
        }
    }

    pub fn states(&self) -> &[LookState] {
        &self.states
    }

    pub fn ignore(&self) -> &[DeclaredName] {
        &self.ignore
    }

    pub fn transitions(&self) -> &[LookTransition] {
        &self.transitions
    }

    /// Each condition its states and its ignore name.
    pub fn conditions(&self) -> impl Iterator<Item = &DeclaredName> {
        let sets = self
            .states
            .iter()
            .flat_map(|state| state.sets.iter().flatten());
        sets.chain(&self.ignore)
    }
}

impl LookState {
    /// The state of `sets` and `models`, of no animation and no key.
    pub const fn new(sets: Vec<Vec<DeclaredName>>, models: Vec<ClientModel>) -> Self {
        Self {
            sets,
            models,
            animation: None,
            key: None,
            waits_for: None,
        }
    }

    /// The top of the highest of its models, of those whose top is known; none when none is.
    pub fn top(&self) -> Option<ModelTop> {
        let tops = self.models.iter().filter_map(|model| model.top);
        tops.reduce(|top, other| if other.get() > top.get() { other } else { top })
    }
}

impl LookTransition {
    /// The transition from the key `from` to the key `to`, of `models` and `animation`, which
    /// plays once; none for a transition from a key to itself, or of an animation of a mode
    /// other than once, as no transition of them ever ends.
    pub fn new(
        from: DeclaredName,
        to: DeclaredName,
        models: Vec<ClientModel>,
        animation: Option<StateAnimation>,
    ) -> Option<Self> {
        let once = animation.as_ref().is_none_or(|animation| {
            matches!(
                animation.mode(),
                AnimationMode::Once | AnimationMode::OnceBackwards
            )
        });
        (from != to && once).then_some(Self {
            from,
            to,
            models,
            animation,
        })
    }

    pub const fn from(&self) -> &DeclaredName {
        &self.from
    }

    pub const fn to(&self) -> &DeclaredName {
        &self.to
    }

    pub fn models(&self) -> &[ClientModel] {
        &self.models
    }

    pub const fn animation(&self) -> Option<&StateAnimation> {
        self.animation.as_ref()
    }
}

/// A package's file is untrusted, so a transition from a key to itself, or of an animation that
/// plays other than once, fails to read.
impl<'de> Deserialize<'de> for LookTransition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            from: DeclaredName,
            to: DeclaredName,
            models: Vec<ClientModel>,
            #[serde(default)]
            animation: Option<StateAnimation>,
        }
        let Fields {
            from,
            to,
            models,
            animation,
        } = Fields::deserialize(deserializer)?;
        Self::new(from, to, models, animation).ok_or_else(|| {
            D::Error::custom("a transition is from its key to itself, or plays other than once")
        })
    }
}

/// A package's file is untrusted, so a draw of no state, of a state of no set of conditions, or
/// of two transitions of one pair of keys fails to read.
impl<'de> Deserialize<'de> for LookDraw {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            states: Vec<LookState>,
            #[serde(default)]
            ignore: Vec<DeclaredName>,
            #[serde(default)]
            transitions: Vec<LookTransition>,
        }
        let Fields {
            states,
            ignore,
            transitions,
        } = Fields::deserialize(deserializer)?;
        Self::new(states, ignore, transitions).ok_or_else(|| {
            D::Error::custom("a draw has no state, a state of no set, or a pair's transition twice")
        })
    }
}
