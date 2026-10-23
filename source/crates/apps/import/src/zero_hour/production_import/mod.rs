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

use crate::zero_hour::axes;
use crate::zero_hour::combat_import::CombatImport;
use crate::zero_hour::dropped::Losses;
use crate::zero_hour::format::csf::string_table::StringTable;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::command_button_ini::{ButtonCommand, CommandButton};
use crate::zero_hour::format::ini::object_ini::IniObject;
use crate::zero_hour::format::ini::object_ini::ini_build::{IniBuild, Prerequisite};
use crate::zero_hour::game_time;
use crate::zero_hour::import_names::{NamedObject, ProductionNames};
use crate::zero_hour::imported_type::ImportedType;
use crate::zero_hour::skirmish_start::FactionStart;
use crate::zero_hour::supply_import::{DOCK_RANGE, SupplyImport};
use crate::zero_hour::type_name::TypeName;
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
    pub(crate) dropped: Losses<ButtonDropCause>,
    /// Each producer's and builder's actions of its slot kind `command`, by its type's name, in
    /// its command set's order.
    pub(crate) commands: BTreeMap<TypeName, Vec<DeclaredName>>,
    /// Each producer's `production` section, by its type's name.
    pub(crate) sections: BTreeMap<TypeName, ProductionData>,
    /// The docks, supply centers, gatherers and free units of the types it visits.
    pub(crate) supply: SupplyImport,
}

/// A faction's base, which a player slot that starts with one gets beside its start
/// (09-zero-hour.md, A base start): each type its start builders build but its faction building,
/// once each, in their command sets' order, and each type those buildings and its faction building
/// train, once each, in that order; its army, those units that neither build nor gather and that a
/// weapon arms, which a bot trains and sends
/// (09-zero-hour.md, A builder and an attacker); its gatherers, those units that gather; and its
/// drop-offs, its faction building and base buildings that take a load, which a bot counts its
/// gatherers by (04-capabilities/control.md, A bot gathers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FactionBase {
    pub(crate) buildings: Vec<DeclaredName>,
    pub(crate) units: Vec<DeclaredName>,
    pub(crate) army: Vec<DeclaredName>,
    pub(crate) gatherers: Vec<DeclaredName>,
    pub(crate) drop_offs: Vec<DeclaredName>,
}

impl FactionBase {
    /// The base of `faction`, from the command sets `production` converted, its army armed by a
    /// weapon `combat` converted.
    pub(crate) fn of(
        production: &ProductionImport,
        combat: &CombatImport,
        faction: &FactionStart,
    ) -> Self {
        let building = &faction.building;
        let mut buildings: Vec<DeclaredName> = Vec::new();
        for builder in &faction.units {
            for made in production.made(builder, ActionKind::Build) {
                if made != building && !buildings.contains(made) {
                    buildings.push(made.clone());
                }
            }
        }
        let mut units: Vec<DeclaredName> = Vec::new();
        for producer in [building].into_iter().chain(&buildings) {
            for made in production.made(producer, ActionKind::Train) {
                if !units.contains(made) {
                    units.push(made.clone());
                }
            }
        }
        let army = units
            .iter()
            .filter(|unit| production.fights(unit) && combat.arms(unit))
            .cloned()
            .collect();
        let gatherers = units
            .iter()
            .filter(|unit| production.gathers(unit))
            .cloned()
            .collect();
        let drop_offs = [building]
            .into_iter()
            .chain(&buildings)
            .filter(|building| production.supply.drop_offs.contains_key(*building))
            .cloned()
            .collect();
        Self {
            buildings,
            units,
            army,
            gatherers,
            drop_offs,
        }
    }
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

/// Why the import drops a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll)]
pub enum ButtonDropCause {
    /// No file defines the button, or its object, or an object its prerequisites name.
    NoObject,
    /// Its object's prerequisites need a science.
    Science,
    /// It trains an object that does not walk, or variations one of which does not.
    NoWalker,
    /// It builds an object of no body, as a small structure (09-zero-hour.md, A cylinder structure
    /// builds as a circle).
    NoBody,
    /// It trains at an object of no `ProductionUpdate`, or of a queue past 1 to 255.
    NoQueue,
    /// Its object, or an object its prerequisites name, gives no unit type.
    NoType,
    /// Its object's build time is below 0, or not finite.
    BadTime,
    /// Its action's name, of its kind and its type's, passes the bytes a declared name holds.
    LongName,
}

/// A button's kind, of those the import converts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Making {
    Train,
    Build,
}

impl ProductionImport {
    /// Whether `unit`'s command set holds a gather.
    fn gathers(&self, unit: &DeclaredName) -> bool {
        let commands = self.commands.get(unit);
        commands
            .into_iter()
            .flatten()
            .any(|id| self.supply.gathers.contains_key(id))
    }

    /// Whether `unit`'s command set holds trains alone: none of its actions builds or gathers.
    fn fights(&self, unit: &DeclaredName) -> bool {
        let commands = self.commands.get(unit);
        commands.into_iter().flatten().all(|id| {
            let action = self.actions.get(id);
            action.is_some_and(|action| matches!(action.made, MadeSection::Train(_)))
        })
    }

    /// The unit types the actions of `kind` of `producer`'s command set make, in its order.
    fn made<'a>(
        &'a self,
        producer: &DeclaredName,
        kind: ActionKind,
    ) -> impl Iterator<Item = &'a DeclaredName> {
        let commands = self.commands.get(producer);
        commands
            .into_iter()
            .flatten()
            .filter_map(|id| self.actions.get(id))
            .filter_map(move |action| match (&action.made, kind) {
                (MadeSection::Train(train), ActionKind::Train) => Some(&train.unit_type),
                (MadeSection::Build(build), ActionKind::Build) => Some(&build.unit_type),
                _ => None,
            })
    }

    /// The trains, builds and supply of `rules`, from the unit types `names` holds, each type a
    /// converted button makes or its variations hold, a building frees, or its object's
    /// prerequisites name, named there, and its own command set's buttons and supply converted in
    /// turn.
    pub(crate) fn of<'r>(rules: &'r ZeroHourRules, names: &mut ProductionNames<'r>) -> Self {
        let mut import = Self::default();
        let mut queued: VecDeque<NamedObject<'r>> = names.typed_objects().into();
        // A type is visited once, by its name, as several parents may field one variation.
        let mut seen: BTreeSet<TypeName> = queued.iter().map(|named| named.name.clone()).collect();
        while let Some(producer) = queued.pop_front() {
            let set = producer.object.build().command_set.as_deref();
            let buttons = set.and_then(|set| rules.sets.buttons(set));
            let mut made: Vec<NamedObject<'r>> = buttons
                .into_iter()
                .flatten()
                .filter_map(|button_name| import.button(rules, &producer, button_name, names))
                .collect();
            made.extend(import.supply_of(rules, &producer, names));
            made.extend(names.take_fresh());
            for named in made {
                if seen.insert(named.name.clone()) {
                    queued.push_back(named);
                }
            }
        }
        import
    }

    /// Converts the supply of `producer`, a unit type of `names`, its buttons converted: its
    /// node, its drop-off, its gather and its hack, the last of its command slots, and the units it frees and
    /// keeps as its production's, a building that spawns and trains nothing taking the production
    /// of its `ProductionUpdate` (09-zero-hour.md, A spawner takes its production update); the
    /// names of the objects it frees and keeps.
    fn supply_of<'r>(
        &mut self,
        rules: &'r ZeroHourRules,
        producer: &NamedObject<'r>,
        names: &mut ProductionNames<'r>,
    ) -> Vec<NamedObject<'r>> {
        self.supply.dock(rules, producer);
        let NamedObject {
            name,
            object: producer,
        } = producer;
        let gather = self.supply.gather(rules, producer, names);
        let hack = self.supply.hack(rules, producer, names);
        for command in [gather, hack].into_iter().flatten() {
            self.commands.entry(name.clone()).or_default().push(command);
        }
        let supply = producer.supply();
        let spawns = supply.kept().is_some() || supply.one_shots().next().is_some();
        if spawns
            && !self.sections.contains_key(name)
            && let Some(section) = Self::section(producer.build())
        {
            self.sections.insert(name.clone(), section);
        }
        let Some(section) = self.sections.get_mut(name) else {
            self.supply.strand(rules, producer);
            return Vec::new();
        };
        let mut freed = self.supply.free(rules, producer, names);
        section.free = freed
            .iter()
            .map(|unit| unit.name.declared().clone())
            .collect();
        let named = NamedObject {
            name: name.clone(),
            object: producer,
        };
        freed.extend(self.supply.keep(rules, &named, names));
        freed
    }

    /// Converts the button `button_name` of the set of `producer`: its action, made once, and
    /// the producer's slot; the name of the object it makes, unless it drops the button or the
    /// button is no train or build.
    fn button<'r>(
        &mut self,
        rules: &'r ZeroHourRules,
        producer: &NamedObject<'r>,
        button_name: &str,
        names: &mut ProductionNames<'r>,
    ) -> Option<NamedObject<'r>> {
        // A button in several sets is reported once for each cause it drops by.
        let mut drop = |cause| {
            self.dropped.drop(button_name, cause);
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
        let Ok(imported) = ImportedType::of(made, rules) else {
            return drop(ButtonDropCause::NoType);
        };
        match making {
            Making::Train if !imported.walks() => return drop(ButtonDropCause::NoWalker),
            Making::Build if !imported.has_body() => return drop(ButtonDropCause::NoBody),
            _ => {}
        }
        let section = match making {
            Making::Train => match Self::section(producer.object.build()) {
                Some(section) => Some(section),
                None => return drop(ButtonDropCause::NoQueue),
            },
            Making::Build => None,
        };
        let Ok(windup_ms) = game_time::of_build_seconds(build.time) else {
            return drop(ButtonDropCause::BadTime);
        };
        let mut units = Vec::with_capacity(requires.len());
        for line in requires {
            let mut any = Vec::with_capacity(line.len());
            for required in line {
                let Ok(typed) = names.unit_type(required, rules) else {
                    return drop(ButtonDropCause::NoType);
                };
                any.push(typed.name.into());
            }
            units.push(RequiredUnit::any_of(any).expect("a prerequisite line names an object"));
        }
        let Ok(typed) = names.unit_type(made, rules) else {
            return drop(ButtonDropCause::NoType);
        };
        let kind = match making {
            Making::Train => ActionKind::Train,
            Making::Build => ActionKind::Build,
        };
        let Ok(id) = typed.name.id(kind.name()) else {
            return drop(ButtonDropCause::LongName);
        };
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
                unit_type: typed.name.declared().clone(),
                requires,
            }),
            Making::Build => MadeSection::Build(BuildData {
                start_life: typed.imported.life().map(Self::one_life),
                unit_type: typed.name.declared().clone(),
                requires,
                construct: ConstructData::Builder,
                start: BuildStart::Order,
                cancel_refund: None,
                placement: None,
            }),
        };
        let money = rules.names.money.clone();
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
                ..ActionKeys::new(targeting)
            };
            ImportedAction {
                keys,
                made: made_section,
            }
        });
        let commands = self.commands.entry(producer.name.clone()).or_default();
        if !commands.contains(&id) {
            commands.push(id);
        }
        if let Some(section) = section {
            self.sections.insert(producer.name.clone(), section);
        }
        Some(NamedObject {
            name: typed.name,
            object: typed.object,
        })
    }

    /// The objects `build`'s prerequisites require, each line the objects any one of which passes
    /// it, as `ProductionPrerequisite::isSatisfied` reads a line (09-zero-hour.md, A prerequisite
    /// line converts to a list); a cause for a line of a science, or of an object no file defines.
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
        let frame = |point: [f32; 2]| {
            let [x, y] = point.map(|axis| game_float::to_num(axis).ok());
            FramePoint::new(axes::ground([x?, y?])?)
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
