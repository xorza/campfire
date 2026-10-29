use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{FlagList, IniLine};
use crate::zero_hour::format::ini::object_ini::ini_animation::IniAnimation;
use crate::zero_hour::format::ini::object_ini::ini_recoil::IniRecoil;
use crate::zero_hour::format::ini::object_ini::{ObjectFiles, SLOTS};
use crate::zero_hour::format::look_conditions::LookConditions;

/// A draw module: how it names its model, the model of its default state, if it draws one, that
/// state's look and animation, and the bones its turret turns and pitches, its `Turret` and its
/// `TurretPitch`; its barrels' recoil; the sets of flags its default state is drawn for, none and
/// its aliases'; its other condition states, in order; the map conditions it ignores
/// (`IgnoreConditionStates`); and its transition states.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DrawModule {
    pub(crate) kind: DrawKind,
    /// Its default state's model, in lowercase.
    pub(crate) model: Option<String>,
    pub(crate) look: StateLook,
    pub(super) animation: IniAnimation,
    pub(crate) turret: Option<String>,
    pub(crate) turret_pitch: Option<String>,
    /// The bone of its object's container it draws at, `AttachToBoneInContainer` of a
    /// `W3DDependencyModelDraw`, in lowercase.
    pub(crate) container_bone: Option<String>,
    /// The weapon slots whose clips its models show the rounds of,
    /// `ProjectileBoneFeedbackEnabledSlots`.
    pub(crate) rounds_shown: [bool; 3],
    pub(crate) recoil: IniRecoil,
    /// The particle systems its treads throw as its object moves, as a `W3DTankDraw`, the
    /// `W3DOverlordTankDraw` that extends it and a `W3DTankTruckDraw` do, `TreadDebrisLeft` and
    /// `TreadDebrisRight`, `TrackDebrisDirtLeft` and `TrackDebrisDirtRight` when they name none;
    /// and the texture of the track marks it lays, its `TrackMarks`.
    pub(crate) tread_dust: Option<[String; 2]>,
    pub(crate) track_marks: Option<String>,
    pub(super) sets: Vec<ConditionSet>,
    pub(super) states: Vec<DrawState>,
    pub(super) ignored: ConditionSet,
    pub(super) transitions: Vec<DrawTransition>,
}

/// A condition state of a `W3DModelDraw` past its default: the sets of flags it is drawn for, its
/// own and its aliases', and its model, look and animation, which start as the default state's
/// when the module has a `DefaultConditionState`.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DrawState {
    pub(super) sets: Vec<ConditionSet>,
    pub(super) model: Option<String>,
    pub(super) look: StateLook,
    pub(super) animation: IniAnimation,
}

/// A `TransitionState` of a `W3DModelDraw`: the keys of the states it is drawn between, from and
/// to, in lowercase, and its model, look and animation, which start as the default state's, of no
/// key, when the module has a `DefaultConditionState`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DrawTransition {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(super) model: Option<String>,
    pub(super) look: StateLook,
    pub(crate) animation: IniAnimation,
}

/// A set of a state's condition flags, each in lowercase, in order and once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ConditionSet {
    flags: Vec<String>,
}

/// A state of a draw module, its default or a later one: the sets of flags it is drawn for, its
/// model and its look, and its animation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ModuleState<'a> {
    pub(crate) sets: &'a [ConditionSet],
    pub(crate) drawn: DrawnState<'a>,
    pub(crate) animation: &'a IniAnimation,
}

/// The model and the look a draw module draws on a map of some conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DrawnState<'a> {
    pub(crate) model: Option<&'a str>,
    pub(crate) look: &'a StateLook,
}

/// What a default state hides and shows, and the bones of its weapons, by slot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct StateLook {
    pub(crate) hide_show: Vec<HideShow>,
    pub(crate) weapons: [WeaponBones; 3],
}

/// A sub-object a state hides or shows, by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HideShow {
    pub(crate) name: String,
    pub(crate) hide: bool,
}

/// A weapon slot's bones, each a base name the game numbers from 01, and the sub-object its
/// `WeaponHideShowBone` names, which shows its clip's rounds as one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WeaponBones {
    pub(crate) fire_fx: Option<String>,
    pub(crate) recoil: Option<String>,
    pub(crate) muzzle_flash: Option<String>,
    pub(crate) launch: Option<String>,
    pub(crate) rounds: Option<String>,
}

/// How a draw module names its model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DrawKind {
    /// The `Model` of its default state, as the `W3DModelDraw` family does.
    Family,
    /// The `ModelName` of a `W3DTreeDraw`, which the client's tree buffer draws.
    Tree,
    /// The `ModelName` of a `W3DPropDraw`.
    Prop,
    /// It draws no model.
    Other,
}

impl DrawModule {
    /// A module of `kind`, of no state yet.
    pub(super) fn new(kind: DrawKind) -> Self {
        Self {
            kind,
            model: None,
            look: StateLook::default(),
            turret: None,
            turret_pitch: None,
            container_bone: None,
            rounds_shown: [false; 3],
            recoil: IniRecoil::default(),
            tread_dust: None,
            track_marks: None,
            animation: IniAnimation::default(),
            sets: vec![ConditionSet::default()],
            states: Vec::new(),
            ignored: ConditionSet::default(),
            transitions: Vec::new(),
        }
    }

    /// The model and the look it draws on a map of `conditions`, those it ignores dropped: of
    /// its state whose set of flags best matches them, as `SparseMatchFinder::findBestInfoSlow`
    /// finds it: the set of the most flags in common, of a tie the one of the fewest the map
    /// lacks, of a tie the first, the states in order, the default first.
    pub(crate) fn drawn(&self, conditions: LookConditions) -> DrawnState<'_> {
        let held = |flag: &str| match flag {
            "night" => conditions.night,
            "snow" => conditions.snow,
            _ => false,
        };
        self.best_state(held).drawn
    }

    /// Its state drawn while the flags `held` holds are set, as `drawn` finds it.
    pub(crate) fn best_state(&self, held: impl Fn(&str) -> bool) -> ModuleState<'_> {
        let held = |flag: &String| held(flag) && !self.ignored.flags.contains(flag);
        let mut best = None;
        let (mut most, mut fewest) = (0, usize::MAX);
        for state in self.states() {
            for set in state.sets {
                let common = set.flags.iter().filter(|flag| held(flag)).count();
                let lacked = set.flags.len() - common;
                if common > most || (common >= most && lacked < fewest) {
                    (best, most, fewest) = (Some(state), common, lacked);
                }
            }
        }
        best.expect("the default state's set is drawn for its first")
    }

    /// Its states, its default first, then the others in order.
    pub(crate) fn states(&self) -> impl Iterator<Item = ModuleState<'_>> {
        let default = ModuleState {
            sets: &self.sets,
            drawn: DrawnState {
                model: self.model.as_deref(),
                look: &self.look,
            },
            animation: &self.animation,
        };
        let states = self.states.iter().map(|state| ModuleState {
            sets: &state.sets,
            drawn: DrawnState {
                model: state.model.as_deref(),
                look: &state.look,
            },
            animation: &state.animation,
        });
        std::iter::once(default).chain(states)
    }

    /// The flags it ignores as it matches its states, `IgnoreConditionStates`.
    pub(crate) fn ignored(&self) -> &[String] {
        &self.ignored.flags
    }

    pub(crate) fn transitions(&self) -> &[DrawTransition] {
        &self.transitions
    }

    /// A field of its default state that the look reads.
    pub(super) fn default_field(
        &mut self,
        word: &str,
        tokens: &[&str],
        line: usize,
    ) -> Result<(), IniError> {
        let bone = match word {
            "turret" => Some(&mut self.turret),
            "turretpitch" => Some(&mut self.turret_pitch),
            _ => None,
        };
        if let (Some(bone), Some(named)) = (bone, tokens.get(1)) {
            *bone = Some((*named).to_owned());
        }
        state_field(&mut self.model, &mut self.look, word, tokens, line)
    }

    /// The state the reader is in past the default, its last.
    pub(super) fn last_state(&mut self) -> &mut DrawState {
        self.states
            .last_mut()
            .expect("a state's block is in its module")
    }
}

impl DrawTransition {
    /// The model and the look it draws.
    pub(crate) fn drawn(&self) -> DrawnState<'_> {
        DrawnState {
            model: self.model.as_deref(),
            look: &self.look,
        }
    }
}

impl ConditionSet {
    /// The set of flags of a state's line `at`, from its second token, as `ModelConditionFlags`
    /// reads them: `NONE` none, a name of either case once however often it is named.
    pub(super) fn of(at: &IniLine<'_>) -> Result<Self, IniError> {
        let names: Vec<&str> = match at.flags()? {
            FlagList::Clear => Vec::new(),
            FlagList::Only(names) => names,
            FlagList::Change(changes) => changes
                .into_iter()
                .filter(|change| change.set)
                .map(|change| change.name)
                .collect(),
        };
        let mut flags: Vec<String> = names.iter().map(|name| name.to_ascii_lowercase()).collect();
        flags.sort_unstable();
        flags.dedup();
        Ok(Self { flags })
    }

    pub(crate) fn flags(&self) -> &[String] {
        &self.flags
    }
}

/// A field of a condition state that the look reads, into its `model` and `look`.
pub(super) fn state_field(
    model: &mut Option<String>,
    look: &mut StateLook,
    word: &str,
    tokens: &[&str],
    line: usize,
) -> Result<(), IniError> {
    match word {
        "model" => *model = ObjectFiles::model(tokens),
        "hidesubobject" | "showsubobject" => {
            look.hide_show(&tokens[1..], word == "hidesubobject");
        }
        "weaponfirefxbone" | "weaponrecoilbone" | "weaponmuzzleflash" | "weaponlaunchbone"
        | "weaponhideshowbone" => {
            let (Some(slot), Some(bone)) = (tokens.get(1), tokens.get(2)) else {
                return Err(IniError::WeaponSlot { line });
            };
            let slot = SLOTS
                .iter()
                .position(|known| slot.eq_ignore_ascii_case(known))
                .ok_or(IniError::WeaponSlot { line })?;
            let bones = &mut look.weapons[slot];
            let field = match word {
                "weaponfirefxbone" => &mut bones.fire_fx,
                "weaponrecoilbone" => &mut bones.recoil,
                "weaponmuzzleflash" => &mut bones.muzzle_flash,
                "weaponhideshowbone" => &mut bones.rounds,
                _ => &mut bones.launch,
            };
            *field = Some((*bone).to_owned()).filter(|bone| !bone.eq_ignore_ascii_case("none"));
        }
        _ => {}
    }
    Ok(())
}

impl StateLook {
    /// Hides or shows `names`, as `parseShowHideSubObject` does: `NONE` first empties the list,
    /// and a name the list holds keeps its place and takes the new change.
    fn hide_show(&mut self, names: &[&str], hide: bool) {
        let list = &mut self.hide_show;
        if names
            .first()
            .is_some_and(|name| name.eq_ignore_ascii_case("none"))
        {
            list.clear();
            return;
        }
        for name in names {
            match list
                .iter_mut()
                .find(|known| known.name.eq_ignore_ascii_case(name))
            {
                Some(known) => known.hide = hide,
                None => list.push(HideShow {
                    name: (*name).to_owned(),
                    hide,
                }),
            }
        }
    }
}
