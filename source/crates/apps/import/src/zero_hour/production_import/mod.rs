use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::num::NonZeroU8;

use campfire_actions::{ActionKeys, ActionKind, ActionRange, RangeField, Targeting};
use campfire_codec::TomlTable;
use campfire_common::{DeclaredName, EnumAll};
use campfire_math::{Num, Rounding};
use campfire_production::{
    BuildData, BuildStart, ConstructData, FramePoint, ProductionData, RequiredUnit, RequiresData,
    TrainData,
};
use campfire_values::{Number, Ranked, Share, TextId};

use crate::zero_hour::format::csf::string_table::StringTable;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::command_button_ini::{ButtonCommand, CommandButton};
use crate::zero_hour::format::ini::object_ini::IniObject;
use crate::zero_hour::format::ini::object_ini::ini_build::{IniBuild, Prerequisite};
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::ProductionNames;
use crate::zero_hour::imported_type::{ImportedType, MONEY};
use crate::zero_hour::supply_import::{DOCK_RANGE, SupplyImport};
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// Zero Hour's trains and builds (09-zero-hour.md, Every command set's trains and builds are
/// imported): each `UNIT_BUILD` and `DOZER_CONSTRUCT` button of the command set of each unit type,
/// and of each type a converted button makes, once for the whole import; the names of their
/// actions, as the string table gives their labels; the buttons dropped, each once for each cause,
/// in the order the sets are read; what it gives each producer, its actions of the slot kind
/// `command` and its `production` section; and the supply of each type it visits.
#[derive(Debug, Default)]
pub(crate) struct ProductionImport {
    pub(crate) actions: BTreeMap<DeclaredName, ImportedAction>,
    pub(crate) texts: BTreeMap<String, String>,
    pub(crate) dropped: Vec<DroppedButton>,
    /// Each producer's and builder's actions of its slot kind `command`, by its type's name, in
    /// its command set's order.
    pub(crate) commands: BTreeMap<ImportName, Vec<DeclaredName>>,
    /// Each producer's `production` section, by its type's name.
    pub(crate) sections: BTreeMap<ImportName, ProductionData>,
    /// The docks, supply centers, gatherers and free units of the types it visits.
    pub(crate) supply: SupplyImport,
}

/// A train or a build as the mode's data writes it, `[actions.<id>]`: the pipeline's keys it
/// gives, and its kind's section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedAction {
    keys: ActionKeys,
    made: MadeSection,
}

/// The section of what a train or a build makes, of its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MadeSection {
    Train(TrainData),
    Build(BuildData),
}

impl ImportedAction {
    /// Its table under `[actions]`.
    pub(crate) fn table(&self) -> TomlTable {
        match &self.made {
            MadeSection::Train(train) => self.keys.table(train),
            MadeSection::Build(build) => self.keys.table(build),
        }
    }
}

/// A button the import drops, by its name, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedButton {
    pub button: String,
    pub cause: ButtonDropCause,
}

/// Why the import drops a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll)]
pub enum ButtonDropCause {
    /// No file defines the button, or its object, or an object its prerequisites name.
    NoObject,
    /// Its object's prerequisites need a science.
    Science,
    /// It trains an object that does not walk.
    NoWalker,
    /// It builds an object of no body, as a small structure (12-base-defenses.md, A cylinder
    /// structure builds as a circle).
    NoBody,
    /// It trains at an object of no `ProductionUpdate`, or of a queue past 1 to 255.
    NoQueue,
    /// Its object, or an object its prerequisites name, gives no unit type.
    NoType,
    /// Its object's build time is below 0, or not finite.
    BadTime,
}

/// A button's kind, of those the import converts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Making {
    Train,
    Build,
}

/// The logic frames a second of the game, `LOGICFRAMES_PER_SECOND`.
const FRAMES: f32 = 30.0;

impl ProductionImport {
    /// The trains, builds and supply of `rules`, from the unit types `names` holds, each type a
    /// converted button makes, a building frees, or its object's prerequisites name, named there,
    /// and its own command set's buttons and supply converted in turn.
    pub(crate) fn of(rules: &ZeroHourRules, names: &mut ProductionNames) -> Self {
        let mut import = Self::default();
        let mut queued: VecDeque<String> = names.typed_objects().into();
        let mut seen: BTreeSet<String> = queued.iter().cloned().collect();
        while let Some(name) = queued.pop_front() {
            let Some(producer) = rules.objects.object(name.as_bytes()) else {
                continue;
            };
            let set = producer.build().command_set.as_deref();
            let buttons = set.and_then(|set| rules.sets.buttons(set));
            let mut made: Vec<String> = buttons
                .into_iter()
                .flatten()
                .filter_map(|button_name| import.button(rules, producer, button_name, names))
                .collect();
            made.extend(import.supply_of(rules, producer, names));
            for object in made {
                if seen.insert(object.clone()) {
                    queued.push_back(object);
                }
            }
        }
        import
    }

    /// Converts the supply of `producer`, a unit type of `names`, its buttons converted: its
    /// node, its drop-off, its gather, the last of its command slots, and the units it frees as
    /// its production's; the names of the objects it frees.
    fn supply_of(
        &mut self,
        rules: &ZeroHourRules,
        producer: &IniObject,
        names: &mut ProductionNames,
    ) -> Vec<String> {
        self.supply.dock(rules, producer);
        let name = ImportName::object(producer);
        if let Some(gather) = self.supply.gather(rules, producer, names) {
            self.commands.entry(name.clone()).or_default().push(gather);
        }
        let Some(section) = self.sections.get_mut(&name) else {
            self.supply.strand(rules, producer);
            return Vec::new();
        };
        let freed = self.supply.free(rules, producer, names);
        section.free = freed.iter().map(|unit| unit.unit_type.clone()).collect();
        freed.into_iter().map(|unit| unit.object).collect()
    }

    /// Converts the button `button_name` of the set of `producer`: its action, made once, and
    /// the producer's slot; the name of the object it makes, unless it drops the button or the
    /// button is no train or build.
    fn button(
        &mut self,
        rules: &ZeroHourRules,
        producer: &IniObject,
        button_name: &str,
        names: &mut ProductionNames,
    ) -> Option<String> {
        // A button in several sets is reported once for each cause it drops by.
        let mut drop = |cause| {
            let dropped = DroppedButton {
                button: button_name.to_owned(),
                cause,
            };
            if !self.dropped.contains(&dropped) {
                self.dropped.push(dropped);
            }
            None
        };
        let Some(button) = rules.buttons.get(button_name) else {
            return drop(ButtonDropCause::NoObject);
        };
        let making = match button.command {
            ButtonCommand::UnitBuild => Making::Train,
            ButtonCommand::DozerConstruct => Making::Build,
            ButtonCommand::Other => return None,
        };
        let made = button
            .object
            .as_deref()
            .and_then(|object| rules.objects.object(object.as_bytes()));
        let Some(made) = made else {
            return drop(ButtonDropCause::NoObject);
        };
        let build = made.build();
        let requires = match Self::requires(rules, build) {
            Ok(requires) => requires,
            Err(cause) => return drop(cause),
        };
        let Ok(imported) = ImportedType::of(made, &rules.locomotors) else {
            return drop(ButtonDropCause::NoType);
        };
        match making {
            Making::Train if !imported.walks() => return drop(ButtonDropCause::NoWalker),
            Making::Build if !imported.has_body() => return drop(ButtonDropCause::NoBody),
            _ => {}
        }
        let section = match making {
            Making::Train => match Self::section(producer.build()) {
                Some(section) => Some(section),
                None => return drop(ButtonDropCause::NoQueue),
            },
            Making::Build => None,
        };
        let Some(windup_ms) = Self::windup_ms(build.time) else {
            return drop(ButtonDropCause::BadTime);
        };
        let mut units = Vec::with_capacity(requires.len());
        for line in requires {
            let mut any = Vec::with_capacity(line.len());
            for required in line {
                let Ok(typed) = names.unit_type(required, &rules.locomotors) else {
                    return drop(ButtonDropCause::NoType);
                };
                any.push(typed.name);
            }
            units.push(RequiredUnit::any_of(any).expect("a prerequisite line names an object"));
        }
        let Ok(typed) = names.unit_type(made, &rules.locomotors) else {
            return drop(ButtonDropCause::NoType);
        };
        let kind = match making {
            Making::Train => ActionKind::Train,
            Making::Build => ActionKind::Build,
        };
        let id = DeclaredName::new(&format!("{}_{}", kind.name(), typed.name))
            .expect("a kind and a type's name make a declared name");
        let message = format!("action-{}", id.as_str().replace('_', "-"));
        self.texts
            .entry(message.clone())
            .or_insert_with(|| Self::text(&rules.strings, button, made));
        let requires = (!units.is_empty()).then(|| RequiresData {
            units,
            modifiers: Vec::new(),
        });
        let made_section = match making {
            Making::Train => MadeSection::Train(TrainData {
                unit_type: typed.name,
                requires,
            }),
            Making::Build => MadeSection::Build(BuildData {
                start_life: typed.imported.life().map(Self::one_life),
                unit_type: typed.name,
                requires,
                construct: ConstructData::Builder,
                start: BuildStart::Order,
                cancel_refund: None,
                placement: None,
            }),
        };
        let money = DeclaredName::new(MONEY).expect("a resource is a declared name");
        let cost = (build.cost > 0).then(|| (money, Self::number(i64::from(build.cost))));
        let windup_ms = i64::try_from(windup_ms).expect("a windup of an `Int` of frames fits");
        self.actions.entry(id.clone()).or_insert_with(|| {
            let targeting = match making {
                Making::Train => Targeting::None,
                Making::Build => Targeting::Point,
            };
            let keys = ActionKeys {
                name: Some(TextId::new(&message).expect("an action's message is a message id")),
                cost: cost.into_iter().collect(),
                windup_ms: Some(Self::number(windup_ms)),
                range: (making == Making::Build)
                    .then(|| Ranked::One(RangeField::Range(ActionRange::meters(DOCK_RANGE)))),
                ..ActionKeys::new(kind, targeting)
            };
            ImportedAction {
                keys,
                made: made_section,
            }
        });
        let commands = self
            .commands
            .entry(ImportName::object(producer))
            .or_default();
        if !commands.contains(&id) {
            commands.push(id);
        }
        if let Some(section) = section {
            self.sections.insert(ImportName::object(producer), section);
        }
        Some(made.name.clone())
    }

    /// The objects `build`'s prerequisites require, each line the objects any one of which
    /// passes it, as `ProductionPrerequisite::isSatisfied` reads a line (12-base-defenses.md, A
    /// prerequisite line converts to a list); a cause for a line of a science, or of an object no
    /// file defines.
    fn requires<'a>(
        rules: &'a ZeroHourRules,
        build: &IniBuild,
    ) -> Result<Vec<Vec<&'a IniObject>>, ButtonDropCause> {
        build
            .prerequisites
            .iter()
            .map(|line| match line {
                Prerequisite::Science(_) => Err(ButtonDropCause::Science),
                Prerequisite::Objects(objects) => objects
                    .iter()
                    .map(|object| {
                        let object = rules.objects.object(object.as_bytes());
                        object.ok_or(ButtonDropCause::NoObject)
                    })
                    .collect(),
            })
            .collect()
    }

    /// The `production` section of a producer of `build`: its queue, its exit's points in the
    /// engine's frame, the game's `y` its `−z`, as a map's places turn, each none for a number the
    /// engine does not hold or a point past its bound, and whether its exit starts its gatherers;
    /// none for no `ProductionUpdate`, or a queue past 1 to 255. The supply's conversion gives it
    /// its free units.
    fn section(build: &IniBuild) -> Option<ProductionData> {
        let queue = u8::try_from(build.queue?).ok().and_then(NonZeroU8::new)?;
        let frame = |[x, y]: [f32; 2]| {
            let x = game_float::to_num(x).ok()?;
            let y = game_float::to_num(y).ok()?;
            FramePoint::new([x, y.checked_neg()?])
        };
        let exit = build.exit;
        Some(ProductionData {
            queue,
            exit: exit.and_then(|exit| frame(exit.create)),
            rally: exit.and_then(|exit| frame(exit.rally)),
            gathers: exit.is_some_and(|exit| exit.starts_gatherers),
            free: Vec::new(),
        })
    }

    /// The share of a life of `most` that is 1 life, as the original's site starts at 1 hit point
    /// (`DozerAIUpdate::construct`): rounded up to the billionths a share holds, so the life it
    /// gives is at least 1 and past it by less than `most / 10⁹`; all of a life of at most 1.
    fn one_life(most: Num) -> Share {
        const BILLION: i128 = 1_000_000_000;
        let bits = i128::from(most.to_bits());
        let one = 1_i128 << Num::FRAC_BITS;
        let billionths = Rounding::Ceiling.divide(BILLION * one, bits).min(BILLION);
        let billionths = u32::try_from(billionths).expect("a share's billionths fit u32");
        Share::billionths(billionths).expect("a share of at most a billion billionths")
    }

    /// `value` at every rank.
    fn number(value: i64) -> Ranked<Number> {
        Ranked::One(Number::from(value))
    }

    /// The milliseconds of a build time of `seconds` the engine turns into the original's count
    /// of frames, `BuildTime × 30` truncated, as `calcTimeToBuild` counts it: rounded up, the
    /// engine's ⌈ms × 30 / 1000⌉ of ⌊frames × 100 / 3⌋ is the count; none for a time below 0, not
    /// finite, or of more frames than the game's `Int` holds, 2³¹ and past, which its cast does
    /// not define.
    fn windup_ms(seconds: f32) -> Option<u64> {
        /// 2³¹, the least count of frames past an `Int`.
        const PAST_INT: f64 = 2_147_483_648.0;
        let frames = seconds * FRAMES;
        // Not a number and either infinity are out of the range too.
        if !(0.0..PAST_INT).contains(&f64::from(frames)) {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the game truncates the frames of a build to an `Int`, toward 0"
        )]
        let frames = frames as u64;
        Some(frames * 100 / 3)
    }

    /// The name of the action of `button`, which makes `made`: the text of its label, its
    /// hotkey's marker dropped, `&&` an `&`; the object's name when the table lacks the label.
    fn text(strings: &StringTable, button: &CommandButton, made: &IniObject) -> String {
        let Some(text) = button.label.as_deref().and_then(|label| strings.get(label)) else {
            return made.name.clone();
        };
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        while let Some(char) = chars.next() {
            match char {
                '&' if chars.peek() == Some(&'&') => {
                    chars.next();
                    out.push('&');
                }
                '&' => {}
                _ => out.push(char),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests;
