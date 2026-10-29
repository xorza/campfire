use campfire_client_files::ModelScale;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;
use crate::zero_hour::format::ini::object_ini::draw_module::{
    ConditionSet, DrawKind, DrawModule, DrawState, DrawTransition, StateLook, state_field,
};
use crate::zero_hour::format::ini::object_ini::fight_data::FightBlock;
use crate::zero_hour::format::ini::object_ini::ini_animation::IniAnimation;
use crate::zero_hour::format::ini::object_ini::ini_build::ProductionModule;
use crate::zero_hour::format::ini::object_ini::ini_contain::IniContain;
use crate::zero_hour::format::ini::object_ini::ini_death::IniDeath;
use crate::zero_hour::format::ini::object_ini::ini_experience::IniStart;
use crate::zero_hour::format::ini::object_ini::ini_object::{IniObject, RadarPriority};
use crate::zero_hour::format::ini::object_ini::ini_power::PowerModule;
use crate::zero_hour::format::ini::object_ini::ini_propaganda::IniPropaganda;
use crate::zero_hour::format::ini::object_ini::ini_supply::SupplyModule;
use crate::zero_hour::format::ini::object_ini::ini_upgrade_module::IniUpgradeModule;
use crate::zero_hour::format::ini::object_ini::{ObjectFiles, SLOTS};

/// The draw modules of the `W3DModelDraw` family, whose default state names the model.
const MODEL_DRAWS: [&str; 11] = [
    "w3dmodeldraw",
    "w3ddependencymodeldraw",
    "w3doverlordaircraftdraw",
    "w3doverlordtankdraw",
    "w3doverlordtruckdraw",
    "w3dpolicecardraw",
    "w3dsciencemodeldraw",
    "w3dsupplydraw",
    "w3dtankdraw",
    "w3dtanktruckdraw",
    "w3dtruckdraw",
];

/// The locomotor set an object moves by, and the one a jet taxis by, of `TheLocomotorSetNames`.
pub(super) const SET_NORMAL: &str = "set_normal";

pub(super) const SET_TAXIING: &str = "set_taxiing";

/// The keywords that open a block wherever they start a line.
pub(super) const OPEN_WITH_VALUE: [&str; 8] = [
    "draw",
    "behavior",
    "body",
    "clientupdate",
    "conditionstate",
    "transitionstate",
    "addmodule",
    "replacemodule",
];

/// The keywords that open a block only alone on their line: with a value, each is a field, as
/// a draw state's `Turret = Turret` is.
pub(super) const OPEN_ALONE: [&str; 14] = [
    "defaultconditionstate",
    "armorset",
    "weaponset",
    "unitspecificsounds",
    "unitspecificfx",
    "prerequisites",
    "turret",
    "altturret",
    "attackareadecal",
    "targetingreticledecal",
    "griddecaltemplate",
    "deliverydecal",
    "inheritablemodule",
    "overrideablebylikekind",
];

/// A block the reader is inside.
#[derive(Debug)]
pub(super) enum ReadBlock {
    Object,
    /// A draw module, with the count of its states begun, and whether its first was a
    /// `DefaultConditionState`, which every later state starts as a copy of.
    Draw {
        kind: DrawKind,
        states: u32,
        copied: bool,
    },
    /// The first state of such a module, its default, and whether it named its model.
    DefaultState {
        named: bool,
    },
    /// A later state of such a module, and whether it holds a model, its own or its copy's.
    State {
        named: bool,
    },
    /// A `TransitionState` of such a module, and whether it holds a model, its own or its copy's.
    Transition {
        named: bool,
    },
    /// An object's `Prerequisites`.
    Prerequisites,
    /// A block combat reads: a body, a weapon set or an armor set.
    Fight(FightBlock),
    /// A behavior module whose fields production reads.
    Production(ProductionModule),
    /// A behavior module whose fields supply reads.
    Supply(SupplyModule),
    /// A special power's or a heal burst's module.
    Power(PowerModule),
    /// An upgrade module, the object's last.
    Upgrade,
    /// A `PhysicsBehavior`, whose mass a wreck reads.
    Physics,
    /// A `LifetimeUpdate`, whose lifetime a wreck reads.
    Lifetime,
    /// A `PropagandaTowerBehavior`, whose aura heals.
    Propaganda,
    /// A `GrantUpgradeCreate`, whose upgrade the object is made with.
    GrantUpgrade,
    /// A `VeterancyGainCreate`, whose level the object is made at.
    VeterancyGain,
    /// A slow death module, the object's last.
    Death,
    /// A contain module, the object's last.
    Contain,
    /// An object's `UnitSpecificSounds`.
    UnitSounds,
    Other,
}

/// A file as it is read: the blocks the reader is inside, the object and the draw module open.
#[derive(Debug, Default)]
pub(super) struct Reading {
    pub(super) stack: Vec<ReadBlock>,
    object: Option<OpenObject>,
    draw: Option<DrawModule>,
}

/// An object being read: whether it reskins another, whose fields its table cannot set, and
/// whether its draw modules are still its base's, which its first own one drops.
#[derive(Debug)]
pub(super) struct OpenObject {
    pub(super) object: IniObject,
    pub(super) reskin: bool,
    pub(super) copied_draws: bool,
}

impl Reading {
    pub(super) fn open(&mut self, object: OpenObject) {
        self.object = Some(object);
        self.stack.push(ReadBlock::Object);
    }

    /// Closes the innermost block; the object it closes, if it is one.
    pub(super) fn end(&mut self, line: usize) -> Result<Option<IniObject>, IniError> {
        match self.stack.pop().ok_or(IniError::StrayEnd { line })? {
            ReadBlock::Object => Ok(Some(
                self.object
                    .take()
                    .expect("an object block holds an object")
                    .object,
            )),
            ReadBlock::Draw { .. } => {
                let done = self.draw.take().expect("a draw block holds a module");
                let open = self.object.as_mut().expect("a draw is in an object");
                if open.copied_draws {
                    open.object.draws.clear();
                    open.copied_draws = false;
                }
                open.object.draws.push(done);
                Ok(None)
            }
            // `parseConditionState` requires each state to name its model, `NONE` or one.
            ReadBlock::DefaultState { named: false }
            | ReadBlock::State { named: false }
            | ReadBlock::Transition { named: false } => Err(IniError::NoModel { line }),
            ReadBlock::DefaultState { named: true } => {
                let module = self.draw.as_ref().expect("a state is in a draw");
                module.animation.check_state(line)?;
                Ok(None)
            }
            ReadBlock::State { named: true } => {
                let module = self.draw.as_ref().expect("a state is in a draw");
                let state = module
                    .states
                    .last()
                    .expect("a state's block is in its module");
                state.animation.check_state(line)?;
                Ok(None)
            }
            ReadBlock::Transition { named: true } => {
                let module = self.draw.as_ref().expect("a transition is in a draw");
                let transition = module
                    .transitions
                    .last()
                    .expect("a transition's block is its own");
                transition.animation.check_transition(line)?;
                Ok(None)
            }
            ReadBlock::Prerequisites
            | ReadBlock::Fight(_)
            | ReadBlock::Production(_)
            | ReadBlock::Supply(_)
            | ReadBlock::Upgrade
            | ReadBlock::Physics
            | ReadBlock::Lifetime
            | ReadBlock::Propaganda
            | ReadBlock::GrantUpgrade
            | ReadBlock::VeterancyGain
            | ReadBlock::Power(_)
            | ReadBlock::Death
            | ReadBlock::Contain
            | ReadBlock::UnitSounds
            | ReadBlock::Other => Ok(None),
        }
    }

    /// A line other than `End`, of its first word in lowercase, `word`.
    pub(super) fn line(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        let (tokens, line) = (at.tokens, at.number);
        let top = self
            .stack
            .last_mut()
            .expect("a line below the top level is in a block");
        match top {
            ReadBlock::Draw {
                kind,
                states,
                copied,
            } => {
                let module = self.draw.as_mut().expect("a draw block holds a module");
                match word {
                    "defaultconditionstate" | "conditionstate" if *kind == DrawKind::Family => {
                        *states += 1;
                        let default = *states == 1;
                        if word == "defaultconditionstate" && !default {
                            return Err(IniError::LateDefaultState { line });
                        }
                        // Without `DefaultConditionState`, the game requires the first state to
                        // set no condition, as `NONE` or no flag says, which is then the default.
                        let no_condition = tokens[1..]
                            .iter()
                            .all(|flag| flag.eq_ignore_ascii_case("none"));
                        if default && word == "conditionstate" && !no_condition {
                            return Err(IniError::NoDefaultState { line });
                        }
                        if default {
                            *copied = word == "defaultconditionstate";
                            self.stack.push(ReadBlock::DefaultState { named: false });
                        } else {
                            // A later state starts as the default's copy, of its own flags.
                            let mut state = DrawState {
                                sets: vec![ConditionSet::of(at)?],
                                model: None,
                                look: StateLook::default(),
                                animation: IniAnimation::default(),
                            };
                            if *copied {
                                state.model.clone_from(&module.model);
                                state.look.clone_from(&module.look);
                                state.animation = module.animation.copied();
                            }
                            let named = state.model.is_some();
                            module.states.push(state);
                            self.stack.push(ReadBlock::State { named });
                        }
                    }
                    "aliasconditionstate" if *kind == DrawKind::Family => {
                        // An alias draws the state before it for one more set of flags.
                        let set = ConditionSet::of(at)?;
                        match (module.states.last_mut(), *states) {
                            (Some(state), _) => state.sets.push(set),
                            (None, 1..) => module.sets.push(set),
                            (None, 0) => return Err(IniError::FirstAlias { line }),
                        }
                    }
                    "transitionstate" if *kind == DrawKind::Family => {
                        // A transition starts as the default's copy, of no key of its own.
                        let (Some(from), Some(to)) = (tokens.get(1), tokens.get(2)) else {
                            return Err(IniError::Transition { line });
                        };
                        let [from, to] = [from, to].map(|key| key.to_ascii_lowercase());
                        if from == to {
                            return Err(IniError::Transition { line });
                        }
                        let mut transition = DrawTransition {
                            from,
                            to,
                            model: None,
                            look: StateLook::default(),
                            animation: IniAnimation::default(),
                        };
                        if *copied {
                            transition.model.clone_from(&module.model);
                            transition.look.clone_from(&module.look);
                            transition.animation = module.animation.copied();
                            transition.animation.key = None;
                            transition.animation.waits_for = None;
                        }
                        let named = transition.model.is_some();
                        // A pair's later transition replaces its earlier, as `m_transitionMap`
                        // takes it.
                        module.transitions.retain(|known| {
                            [&known.from, &known.to] != [&transition.from, &transition.to]
                        });
                        module.transitions.push(transition);
                        self.stack.push(ReadBlock::Transition { named });
                    }
                    _ if *kind == DrawKind::Family && module.recoil.field(word, at)? => {}
                    "ignoreconditionstates" if *kind == DrawKind::Family => {
                        module.ignored = ConditionSet::of(at)?;
                    }
                    "projectilebonefeedbackenabledslots" if *kind == DrawKind::Family => {
                        let mut shown = [false; 3];
                        for slot in &tokens[1..] {
                            let at_slot = SLOTS
                                .iter()
                                .position(|known| slot.eq_ignore_ascii_case(known));
                            shown[at_slot.ok_or(IniError::WeaponSlot { line })?] = true;
                        }
                        module.rounds_shown = shown;
                    }
                    "attachtoboneincontainer" if *kind == DrawKind::Family => {
                        module.container_bone = Some(at.value()?.to_ascii_lowercase());
                    }
                    "trackmarks" if *kind == DrawKind::Family => {
                        let texture = at.value()?;
                        module.track_marks =
                            (!texture.eq_ignore_ascii_case("none")).then(|| texture.to_owned());
                    }
                    "treaddebrisleft" | "treaddebrisright" if module.tread_dust.is_some() => {
                        let side = usize::from(word == "treaddebrisright");
                        let systems = module.tread_dust.as_mut().expect("a tank draw's");
                        at.value()?.clone_into(&mut systems[side]);
                    }
                    "modelname" if matches!(*kind, DrawKind::Tree | DrawKind::Prop) => {
                        module.model = ObjectFiles::model(tokens);
                    }
                    _ if ObjectFiles::opens(word, tokens) => self.stack.push(ReadBlock::Other),
                    _ => {}
                }
            }
            ReadBlock::State { named } => {
                let module = self.draw.as_mut().expect("a state is in a draw");
                *named |= word == "model";
                let state = module.last_state();
                if !state.animation.field(word, at)? {
                    state_field(&mut state.model, &mut state.look, word, tokens, line)?;
                }
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(ReadBlock::Other);
                }
            }
            ReadBlock::DefaultState { named } => {
                let module = self.draw.as_mut().expect("a state is in a draw");
                *named |= word == "model";
                if !module.animation.field(word, at)? {
                    module.default_field(word, tokens, line)?;
                }
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(ReadBlock::Other);
                }
            }
            ReadBlock::Transition { named } => {
                let module = self.draw.as_mut().expect("a transition is in a draw");
                *named |= word == "model";
                let transition = module
                    .transitions
                    .last_mut()
                    .expect("a transition's block is its own");
                if !transition.animation.field(word, at)? {
                    let (model, look) = (&mut transition.model, &mut transition.look);
                    state_field(model, look, word, tokens, line)?;
                }
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(ReadBlock::Other);
                }
            }
            ReadBlock::Object if word == "draw" => {
                let module = tokens
                    .get(1)
                    .map_or_default(|kind| kind.to_ascii_lowercase());
                let kind = match module.as_str() {
                    family if MODEL_DRAWS.contains(&family) => DrawKind::Family,
                    "w3dtreedraw" => DrawKind::Tree,
                    "w3dpropdraw" => DrawKind::Prop,
                    _ => DrawKind::Other,
                };
                let mut draw = DrawModule::new(kind);
                let tank = matches!(
                    module.as_str(),
                    "w3dtankdraw" | "w3doverlordtankdraw" | "w3dtanktruckdraw"
                );
                draw.tread_dust = tank
                    .then(|| ["TrackDebrisDirtLeft", "TrackDebrisDirtRight"].map(str::to_owned));
                self.draw = Some(draw);
                self.stack.push(ReadBlock::Draw {
                    kind,
                    states: 0,
                    copied: false,
                });
            }
            ReadBlock::Prerequisites => {
                let open = self.object.as_mut().expect("prerequisites are an object's");
                open.object.build.prerequisite(word, at)?;
            }
            ReadBlock::Fight(block) => {
                let block = *block;
                let open = self.object.as_mut().expect("a combat block is an object's");
                match open.object.fight.field(block, word, at)? {
                    Some(inner) => self.stack.push(ReadBlock::Fight(inner)),
                    None if ObjectFiles::opens(word, tokens) => self.stack.push(ReadBlock::Other),
                    None => {}
                }
            }
            ReadBlock::Production(module) => {
                let module = *module;
                let open = self.object.as_mut().expect("a module is an object's");
                open.object.build.module_field(module, word, at)?;
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(ReadBlock::Other);
                }
            }
            ReadBlock::Upgrade => {
                let open = self.object.as_mut().expect("a module is an object's");
                let module = open.object.upgrades.last_mut();
                module
                    .expect("an upgrade block opened its module")
                    .field(word, at)?;
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(ReadBlock::Other);
                }
            }
            ReadBlock::Power(module) => {
                let module = *module;
                let open = self.object.as_mut().expect("a module is an object's");
                open.object.powers.module_field(module, word, at)?;
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(ReadBlock::Other);
                }
            }
            ReadBlock::VeterancyGain => {
                let open = self.object.as_mut().expect("a module is an object's");
                open.object.experience.start_field(word, at)?;
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(ReadBlock::Other);
                }
            }
            ReadBlock::GrantUpgrade => {
                let open = self.object.as_mut().expect("a module is an object's");
                if word == "upgradetogrant" {
                    open.object.granted.push(at.value()?.to_owned());
                }
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(ReadBlock::Other);
                }
            }
            ReadBlock::Propaganda => {
                let open = self.object.as_mut().expect("a module is an object's");
                let propaganda = open.object.propaganda.as_mut();
                let propaganda = propaganda.expect("a propaganda block opened its module");
                if !propaganda.field(word, at)? && ObjectFiles::opens(word, tokens) {
                    self.stack.push(ReadBlock::Other);
                }
            }
            ReadBlock::Physics | ReadBlock::Lifetime => {
                let lifetime = matches!(self.stack.last(), Some(ReadBlock::Lifetime));
                let open = self.object.as_mut().expect("a module is an object's");
                let object = &mut open.object;
                match word {
                    "mass" if !lifetime => object.mass = at.real()?,
                    "lateralfriction" if !lifetime => object.lateral_friction = Some(at.real()?),
                    "minlifetime" | "maxlifetime" if lifetime => {
                        let ms = object.lifetime_ms.get_or_insert([0.0; 2]);
                        ms[usize::from(word == "maxlifetime")] = at.real()?;
                    }
                    _ if ObjectFiles::opens(word, tokens) => self.stack.push(ReadBlock::Other),
                    _ => {}
                }
            }
            ReadBlock::Death => {
                let open = self.object.as_mut().expect("a module is an object's");
                let module = open.object.deaths.last_mut();
                module
                    .expect("a death block opened its module")
                    .field(word, at)?;
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(ReadBlock::Other);
                }
            }
            ReadBlock::Supply(module) => {
                let module = *module;
                let open = self.object.as_mut().expect("a module is an object's");
                open.object.supply.module_field(module, word, at)?;
                // A gatherer's or a hacker's module is an AI module, whose turret combat reads.
                let inner = match module {
                    SupplyModule::Gatherer | SupplyModule::Hacker => {
                        open.object.fight.field(FightBlock::Ai, word, at)?
                    }
                    SupplyModule::Dock | SupplyModule::Spawn | SupplyModule::Deposit => None,
                };
                match inner {
                    Some(inner) => self.stack.push(ReadBlock::Fight(inner)),
                    None if ObjectFiles::opens(word, tokens) => self.stack.push(ReadBlock::Other),
                    None => {}
                }
            }
            ReadBlock::Object if !self.object.as_ref().is_some_and(|open| open.reskin) => {
                let open = self
                    .object
                    .as_mut()
                    .expect("an object block holds an object");
                let build = &mut open.object.build;
                // A reskin's table sets none of these, so it keeps its base's.
                if build.field(word, at)? {
                    return Ok(());
                }
                let module = tokens.get(1).map(|module| module.to_ascii_lowercase());
                if word == "behavior"
                    && let Some(module) = module
                        .as_deref()
                        .and_then(|module| open.object.supply.module(module))
                {
                    self.stack.push(ReadBlock::Supply(module));
                    return Ok(());
                }
                if word == "behavior"
                    && let Some(module) = module
                        .as_deref()
                        .and_then(|module| open.object.powers.module(module))
                {
                    self.stack.push(ReadBlock::Power(module));
                    return Ok(());
                }
                if word == "behavior"
                    && let Some(module) = tokens
                        .get(1)
                        .and_then(|class| IniUpgradeModule::module(class))
                {
                    open.object.upgrades.push(module);
                    self.stack.push(ReadBlock::Upgrade);
                    return Ok(());
                }
                if word == "behavior"
                    && let Some(block) = module.as_deref().and_then(|module| match module {
                        "physicsbehavior" => Some(ReadBlock::Physics),
                        "lifetimeupdate" => Some(ReadBlock::Lifetime),
                        "propagandatowerbehavior" => {
                            open.object.propaganda = Some(IniPropaganda::default());
                            Some(ReadBlock::Propaganda)
                        }
                        "grantupgradecreate" => Some(ReadBlock::GrantUpgrade),
                        "veterancygaincreate" => {
                            open.object.experience.starts.push(IniStart::default());
                            Some(ReadBlock::VeterancyGain)
                        }
                        _ => None,
                    })
                {
                    self.stack.push(block);
                    return Ok(());
                }
                if word == "behavior"
                    && let Some(contain) = tokens.get(1).and_then(|class| IniContain::module(class))
                {
                    open.object.contain = Some(contain);
                    self.stack.push(ReadBlock::Contain);
                    return Ok(());
                }
                if word == "behavior"
                    && let Some(module) = tokens.get(1).and_then(|class| IniDeath::module(class))
                {
                    open.object.deaths.push(module);
                    self.stack.push(ReadBlock::Death);
                    return Ok(());
                }
                if let Some(block) = open.object.fight.open(word, tokens) {
                    self.stack.push(ReadBlock::Fight(block));
                    return Ok(());
                }
                if open.object.voices.field(word, at)? {
                    return Ok(());
                }
                if word == "unitspecificsounds" && tokens.len() == 1 {
                    self.stack.push(ReadBlock::UnitSounds);
                    return Ok(());
                }
                if word == "prerequisites" && tokens.len() == 1 {
                    // `parsePrerequisites` starts the list again.
                    build.prerequisites.clear();
                    self.stack.push(ReadBlock::Prerequisites);
                    return Ok(());
                }
                if word == "behavior"
                    && let Some(module) = module.and_then(|module| build.module(&module))
                {
                    self.stack.push(ReadBlock::Production(module));
                    return Ok(());
                }
                self.object_field(word, at)?;
            }
            ReadBlock::Contain => {
                let open = self.object.as_mut().expect("a module is an object's");
                let contain = open.object.contain.as_mut();
                contain
                    .expect("a contain block opened its module")
                    .field(word, at)?;
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(ReadBlock::Other);
                }
            }
            ReadBlock::UnitSounds => {
                let open = self.object.as_mut().expect("a block is an object's");
                open.object.voices.unit_specific(word, at)?;
            }
            ReadBlock::Object => self.object_field(word, at)?,
            ReadBlock::Other if ObjectFiles::opens(word, tokens) => {
                self.stack.push(ReadBlock::Other);
            }
            ReadBlock::Other => {}
        }
        Ok(())
    }

    /// A line of an object's own block: a field of its kinds, fence, scale, bridge, locomotors,
    /// sight or geometry, or a line that opens a block.
    fn object_field(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        let (tokens, line) = (at.tokens, at.number);
        match word {
            _ if matches!(
                word,
                "kindof"
                    | "fencewidth"
                    | "scale"
                    | "isbridge"
                    | "locomotor"
                    | "visionrange"
                    | "shroudclearingrange"
                    | "radarpriority"
                    | "experiencevalue"
                    | "experiencerequired"
                    | "istrainable"
                    | "transportslotcount"
                    | "buttonimage"
            ) || word.starts_with("geometry") =>
            {
                let open = self
                    .object
                    .as_mut()
                    .expect("an object block holds an object");
                let object = &mut open.object;
                // A reskin's table sets its fence's width and its geometry; the game's release
                // build skips a field the table lacks, as its kinds, its scale, whether it is a
                // bridge and its locomotors.
                if object.geometry.field(word, at)? {
                    return Ok(());
                }
                match word {
                    "fencewidth" => object.fence_width = at.real()?,
                    _ if open.reskin => {}
                    "kindof" => object.kinds.set_by(at)?,
                    "isbridge" => object.is_bridge = at.yes()?,
                    "transportslotcount" => object.transport_slots = at.int()?,
                    "buttonimage" => object.button_image = Some(at.value()?.to_owned()),
                    "visionrange" => object.vision_range = at.real()?,
                    "experiencevalue" | "experiencerequired" | "istrainable" => {
                        object.experience.field(word, at)?;
                    }
                    "shroudclearingrange" => object.shroud_clearing_range = at.real()?,
                    "radarpriority" => {
                        object.radar_priority = match at.value()?.to_ascii_uppercase().as_str() {
                            "INVALID" => RadarPriority::Unset,
                            "NOT_ON_RADAR" => RadarPriority::NotOnRadar,
                            "LOCAL_UNIT_ONLY" => RadarPriority::LocalUnitOnly,
                            "STRUCTURE" | "UNIT" => RadarPriority::Shown,
                            // As `RadarPriorityNames` lists no other.
                            _ => return Err(IniError::NotListed { line: at.number }),
                        };
                    }
                    "locomotor" => {
                        let set = at.value()?;
                        // `scanIndexList` finds a set of either case.
                        let names = tokens[2..]
                            .iter()
                            .filter(|name| !name.eq_ignore_ascii_case("none"))
                            .map(|&name| name.to_owned());
                        if set.eq_ignore_ascii_case(SET_NORMAL) {
                            object.locomotors = names.collect();
                        } else if set.eq_ignore_ascii_case(SET_TAXIING) {
                            object.taxi_locomotors = names.collect();
                        }
                    }
                    "scale" => {
                        let scale = ModelScale::new(at.real()?);
                        object.scale = scale.ok_or(IniError::Scale { line })?;
                    }
                    _ => {}
                }
            }
            _ if ObjectFiles::opens(word, tokens) => self.stack.push(ReadBlock::Other),
            _ => {}
        }
        Ok(())
    }
}
