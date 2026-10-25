use campfire_common::DeclaredName;
use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::client_model::{ClientModel, ModelTop};

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
}

/// A state of a draw: the sets of condition names it is drawn for, each a set its unit may hold,
/// and its models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookState {
    pub sets: Vec<Vec<DeclaredName>>,
    pub models: Vec<ClientModel>,
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
    /// The draw of `states`, its default first, that ignores `ignore`; none for no state, or for
    /// a state of no set of conditions, which no unit draws.
    pub fn new(states: Vec<LookState>, ignore: Vec<DeclaredName>) -> Option<Self> {
        let drawn = !states.is_empty() && states.iter().all(|state| !state.sets.is_empty());
        drawn.then_some(Self { states, ignore })
    }

    /// The draw of `models` alone, drawn for no condition.
    pub fn still(models: Vec<ClientModel>) -> Self {
        Self {
            states: vec![LookState {
                sets: vec![Vec::new()],
                models,
            }],
            ignore: Vec::new(),
        }
    }

    pub fn states(&self) -> &[LookState] {
        &self.states
    }

    pub fn ignore(&self) -> &[DeclaredName] {
        &self.ignore
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
    /// The top of the highest of its models, of those whose top is known; none when none is.
    pub fn top(&self) -> Option<ModelTop> {
        let tops = self.models.iter().filter_map(|model| model.top);
        tops.reduce(|top, other| if other.get() > top.get() { other } else { top })
    }
}

/// A package's file is untrusted, so a draw of no state, or of a state of no set of conditions,
/// fails to read.
impl<'de> Deserialize<'de> for LookDraw {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            states: Vec<LookState>,
            #[serde(default)]
            ignore: Vec<DeclaredName>,
        }
        let Fields { states, ignore } = Fields::deserialize(deserializer)?;
        Self::new(states, ignore)
            .ok_or_else(|| D::Error::custom("a draw has no state, or a state of no set"))
    }
}
