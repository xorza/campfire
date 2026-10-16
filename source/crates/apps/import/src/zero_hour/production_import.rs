use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::num::NonZeroU8;

use campfire_derive::EnumAll;
use campfire_math::{Num, Rounding};
use campfire_values::DeclaredName;
use serde::Serialize;

use crate::zero_hour::format::csf::string_table::StringTable;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::command_button_ini::{
    ButtonCommand, CommandButton, CommandButtonIni,
};
use crate::zero_hour::format::ini::command_set_ini::CommandSetIni;
use crate::zero_hour::format::ini::locomotor_ini::LocomotorIni;
use crate::zero_hour::format::ini::object_ini::build_data::{BuildData, Prerequisite};
use crate::zero_hour::format::ini::object_ini::{IniObject, ObjectIni};
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::imported_type::ImportedType;
use crate::zero_hour::unit_types::ProductionSection;

/// What the production import reads: the objects, their command sets and buttons, the
/// locomotors that decide whether a type walks, and the string table of the buttons' names.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CommandSources<'a> {
    pub(crate) ini: &'a ObjectIni,
    pub(crate) sets: &'a CommandSetIni,
    pub(crate) buttons: &'a CommandButtonIni,
    pub(crate) locomotors: &'a LocomotorIni,
    pub(crate) strings: &'a StringTable,
}

/// Zero Hour's trains and builds (12-zero-hour.md, Every command set's trains and builds are
/// imported; 12-zero-hour.md, The default money): each `UNIT_BUILD` and `DOZER_CONSTRUCT` button of
/// the command set of each unit type, and of each type a converted button makes, once for the whole
/// import; the names of their actions, as the string table gives their labels; the buttons dropped,
/// each once for each cause, in the order the sets are read; and what it gives each producer, its
/// actions of the slot kind `command` and its `production` section.
#[derive(Debug, Default)]
pub(crate) struct ProductionImport {
    pub(crate) actions: BTreeMap<DeclaredName, ImportedAction>,
    pub(crate) texts: BTreeMap<String, String>,
    pub(crate) dropped: Vec<DroppedButton>,
    /// Each producer's and builder's actions of its slot kind `command`, by its type's name, in
    /// its command set's order.
    pub(crate) commands: BTreeMap<ImportName, Vec<DeclaredName>>,
    /// Each producer's `production` section, by its type's name.
    pub(crate) sections: BTreeMap<ImportName, ProductionSection>,
}

/// A train or a build as the mode's data writes it, `[actions.<id>]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ImportedAction {
    name: String,
    kind: &'static str,
    targeting: &'static str,
    unit_type: DeclaredName,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    cost: BTreeMap<&'static str, u16>,
    windup_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    requires: Option<Requires>,
    #[serde(skip_serializing_if = "Option::is_none")]
    range: Option<Num>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    construct: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_life: Option<String>,
}

/// What an action requires, its unit types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Requires {
    units: Vec<DeclaredName>,
}

/// A button the import drops, by its name, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedButton {
    pub button: String,
    pub cause: DropCause,
}

/// Why the import drops a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll)]
pub enum DropCause {
    /// No file defines the button, or its object, or an object its prerequisites name.
    NoObject,
    /// Its object's prerequisites need a science.
    Science,
    /// Its object's prerequisites hold a line of several objects, any one of which passes.
    EitherOr,
    /// It trains an object that does not walk.
    NoWalker,
    /// It builds an object of no box body.
    NoBox,
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

/// The slot kind of the converted buttons, of every producer's and builder's.
pub(crate) const COMMAND: &str = "command";

/// The player resource every cost is in.
pub(crate) const MONEY: &str = "money";

/// How near a builder comes to its building's box, 10 m, the pathfinder's cell: the dozer docks in
/// the free cell beside the building nearest it (12-zero-hour.md, Every command set's trains and
/// builds are imported).
const BUILD_RANGE: Num = Num::int(10);

/// The logic frames a second of the game, `LOGICFRAMES_PER_SECOND`.
const FRAMES: f32 = 30.0;

impl ProductionImport {
    /// The trains and builds of `sources`, from the unit types `names` holds, each type a
    /// converted button makes, or its object's prerequisites name, named there, and its own
    /// command set's buttons converted in turn.
    pub(crate) fn of(sources: CommandSources<'_>, names: &mut ImportNames) -> Self {
        let mut import = Self::default();
        let mut queued: VecDeque<String> = names.typed_objects().into();
        let mut seen: BTreeSet<String> = queued.iter().cloned().collect();
        while let Some(name) = queued.pop_front() {
            let Some(producer) = sources.ini.object(name.as_bytes()) else {
                continue;
            };
            let Some(set) = producer.build().command_set.as_deref() else {
                continue;
            };
            let Some(buttons) = sources.sets.buttons(set) else {
                continue;
            };
            for button_name in buttons {
                let made = import.button(sources, producer, button_name, names);
                if let Some(object) = made
                    && seen.insert(object.clone())
                {
                    queued.push_back(object);
                }
            }
        }
        import
    }

    /// Converts the button `button_name` of the set of `producer`: its action, made once, and
    /// the producer's slot; the name of the object it makes, unless it drops the button or the
    /// button is no train or build.
    fn button(
        &mut self,
        sources: CommandSources<'_>,
        producer: &IniObject,
        button_name: &str,
        names: &mut ImportNames,
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
        let Some(button) = sources.buttons.get(button_name) else {
            return drop(DropCause::NoObject);
        };
        let making = match button.command {
            ButtonCommand::UnitBuild => Making::Train,
            ButtonCommand::DozerConstruct => Making::Build,
            ButtonCommand::Other => return None,
        };
        let made = button
            .object
            .as_deref()
            .and_then(|object| sources.ini.object(object.as_bytes()));
        let Some(made) = made else {
            return drop(DropCause::NoObject);
        };
        let build = made.build();
        let requires = match Self::requires(sources, build) {
            Ok(requires) => requires,
            Err(cause) => return drop(cause),
        };
        let Ok(imported) = ImportedType::of(made, sources.locomotors) else {
            return drop(DropCause::NoType);
        };
        match making {
            Making::Train if !imported.walks() => return drop(DropCause::NoWalker),
            Making::Build if !imported.is_box() => return drop(DropCause::NoBox),
            _ => {}
        }
        let section = match making {
            Making::Train => match Self::section(producer.build()) {
                Some(section) => Some(section),
                None => return drop(DropCause::NoQueue),
            },
            Making::Build => None,
        };
        let Some(windup_ms) = Self::windup_ms(build.time) else {
            return drop(DropCause::BadTime);
        };
        let mut units = Vec::with_capacity(requires.len());
        for required in requires {
            let Ok(typed) = names.unit_type(required, sources.locomotors) else {
                return drop(DropCause::NoType);
            };
            units.push(typed.name);
        }
        let Ok(typed) = names.unit_type(made, sources.locomotors) else {
            return drop(DropCause::NoType);
        };
        let kind = match making {
            Making::Train => "train",
            Making::Build => "build",
        };
        let id = DeclaredName::new(&format!("{kind}_{}", typed.name))
            .expect("a kind and a type's name make a declared name");
        let message = format!("action-{}", id.as_str().replace('_', "-"));
        self.texts
            .entry(message.clone())
            .or_insert_with(|| Self::text(sources.strings, button, made));
        self.actions
            .entry(id.clone())
            .or_insert_with(|| ImportedAction {
                name: message,
                kind,
                targeting: match making {
                    Making::Train => "none",
                    Making::Build => "point",
                },
                unit_type: typed.name,
                cost: (build.cost > 0)
                    .then_some((MONEY, build.cost))
                    .into_iter()
                    .collect(),
                windup_ms,
                requires: (!units.is_empty()).then_some(Requires { units }),
                range: (making == Making::Build).then_some(BUILD_RANGE),
                start: (making == Making::Build).then_some("order"),
                construct: (making == Making::Build).then_some("builder"),
                start_life: typed
                    .imported
                    .life()
                    .filter(|_| making == Making::Build)
                    .map(Self::one_life),
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

    /// The objects `build`'s prerequisites require, a line of one object each; a cause for a
    /// line of a science, of several objects, or of an object no file defines.
    fn requires<'a>(
        sources: CommandSources<'a>,
        build: &BuildData,
    ) -> Result<Vec<&'a IniObject>, DropCause> {
        build
            .prerequisites
            .iter()
            .map(|line| match line {
                Prerequisite::Science(_) => Err(DropCause::Science),
                Prerequisite::Objects(objects) => match objects.as_slice() {
                    [object] => sources
                        .ini
                        .object(object.as_bytes())
                        .ok_or(DropCause::NoObject),
                    _ => Err(DropCause::EitherOr),
                },
            })
            .collect()
    }

    /// The `production` section of a producer of `build`: its queue, and its exit's points in
    /// the engine's frame, the game's `y` its `−z`, as a map's places turn; none for no
    /// `ProductionUpdate`, or a queue past 1 to 255.
    fn section(build: &BuildData) -> Option<ProductionSection> {
        let queue = u8::try_from(build.queue?).ok().and_then(NonZeroU8::new)?;
        let frame = |[x, y]: [f32; 2]| {
            let x = game_float::to_num(x).ok()?;
            let y = game_float::to_num(y).ok()?;
            Some([x, y.checked_neg()?])
        };
        let exit = build.exit;
        Some(ProductionSection {
            queue,
            exit: exit.and_then(|exit| frame(exit.create)),
            rally: exit.and_then(|exit| frame(exit.rally)),
        })
    }

    /// The share of a life of `most` that is 1 life, as the original's site starts at 1 hit point
    /// (`DozerAIUpdate::construct`): rounded up to the billionths a share holds, so the life it
    /// gives is at least 1 and past it by less than `most / 10⁹`; all of a life of at most 1.
    fn one_life(most: Num) -> String {
        const BILLION: i128 = 1_000_000_000;
        let bits = i128::from(most.to_bits());
        let one = 1_i128 << Num::FRAC_BITS;
        let billionths = Rounding::Ceiling.divide(BILLION * one, bits);
        if billionths >= BILLION {
            return "1".to_owned();
        }
        format!("0.{billionths:09}")
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
mod tests {
    use super::*;

    #[test]
    fn a_sites_start_is_the_least_share_of_billionths_that_gives_one_life() {
        // 1 of 5,000 is 0.0002 exactly; of 1,200, 0.000833333… up to 833,334 billionths, which
        // gives 1.0000008; of 180, 5,555,555.5… up to 5,555,556; a life of 1 or less starts
        // whole.
        let share = |most: Num| ProductionImport::one_life(most);
        assert_eq!(share(Num::int(5000)), "0.000200000");
        assert_eq!(share(Num::int(1200)), "0.000833334");
        assert_eq!(share(Num::int(180)), "0.005555556");
        assert_eq!(share(Num::ONE), "1");
        assert_eq!(share(Num::HALF), "1");
    }

    #[test]
    #[expect(
        clippy::lossy_float_literal,
        reason = "71,582,784 and 71,582,792 are multiples of 8, which an f32 of 2²⁶ to 2²⁷ holds exactly"
    )]
    fn a_build_time_is_its_frames_in_milliseconds_within_the_games_int() {
        // 5 s is 150 frames, ⌊150 × 100 / 3⌋ = 5,000 ms; 0.1 s is 3 frames, 100 ms. 71,582,784 s
        // is 2,147,483,520 frames, the most an `f32` holds below 2³¹, ⌊2,147,483,520 × 100 / 3⌋
        // = 71,582,784,000 ms; 71,582,792 s, the next time an `f32` holds, is 2³¹ frames, past
        // the game's `Int`, and gives none, as what is not a time at all does.
        let windup = ProductionImport::windup_ms;
        assert_eq!(windup(5.0), Some(5000));
        assert_eq!(windup(0.1), Some(100));
        assert_eq!(windup(0.0), Some(0));
        assert_eq!(windup(71_582_784.0), Some(71_582_784_000));
        for refused in [71_582_792.0, f32::MAX, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(windup(refused), None, "{refused}");
        }
    }
}
