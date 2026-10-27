use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::num::NonZeroU8;

use campfire_actions::{ActionKeys, ActionKind, ActionRange, RangeField, Targeting};
use campfire_airfields::{AirfieldData, AirfieldPlace, RunwayData};
use campfire_codec::TomlTable;
use campfire_common::{DeclaredName, EnumAll};
use campfire_math::{Num, Rounding};
use campfire_production::{
    BuildData, BuildStart, ConstructData, FramePoint, PlaceData, ProductionData, RequiredUnit,
    RequiresData, ResearchData, ResearchHolder, SellData, TrainData,
};
use campfire_values::{Number, Ranked, Share, TextId};
use serde::{Deserialize, Serialize};

use crate::zero_hour::airfield_bones::{AirfieldBones, PlaceBones};
use crate::zero_hour::axes;
use crate::zero_hour::combat_import::CombatImport;
use crate::zero_hour::dropped::Losses;
use crate::zero_hour::format::csf::string_table::StringTable;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::command_button_ini::{ButtonCommand, CommandButton};
use crate::zero_hour::format::ini::object_ini::IniObject;
use crate::zero_hour::format::ini::object_ini::ini_build::{IniBuild, Prerequisite};
use crate::zero_hour::format::ini::upgrade_ini::UpgradeKind;
use crate::zero_hour::game_time;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::{NamedObject, ProductionNames};
use crate::zero_hour::imported_type::ImportedType;
use crate::zero_hour::panel_import::PanelButtons;
use crate::zero_hour::skirmish_start::FactionStart;
use crate::zero_hour::supply_import::{DOCK_RANGE, Spawning, SupplyImport};
use crate::zero_hour::type_name::TypeName;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// Zero Hour's trains, builds and researches (09-zero-hour.md, Every command set's trains and
/// builds are imported; 04-capabilities/production.md, Research is an action kind of production):
/// each `UNIT_BUILD`, `DOZER_CONSTRUCT`, `PLAYER_UPGRADE` and `OBJECT_UPGRADE` button of the
/// command set of each unit type, and of each type a converted button makes, once for the whole
/// import; the names of their actions, as the string table gives their labels; the buttons dropped,
/// each once for each cause, in the order the sets are read; what it gives each producer, its
/// actions of the slot kind `command` and its `production` section; and the supply of each type it
/// visits.
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
    /// The `sell` section of each type whose command set holds a sale, by its type's name.
    pub(crate) sells: BTreeMap<TypeName, SellData>,
    /// The docks, supply centers, gatherers and free units of the types it visits.
    pub(crate) supply: SupplyImport,
    /// The places of each producer's buttons on its panel, and their faces.
    pub(crate) panel: PanelButtons,
    /// The helipad of each type whose model names one, by its type's name, which an airfield's
    /// trains leave from, and each airfield's places and runways, which its jets take.
    helipads: BTreeMap<DeclaredName, [f32; 2]>,
    airfield_bones: BTreeMap<DeclaredName, AirfieldBones>,
    /// The `HealAmountPerSecond` of each producer of a `ParkingPlaceBehavior` it visits.
    heals: BTreeMap<TypeName, f32>,
    /// The `airfield` of each airfield whose production makes its jets at places (09-zero-hour.md,
    /// The import flies a winged locomotor).
    pub(crate) airfields: BTreeMap<TypeName, AirfieldData>,
}

/// Where a producer's units leave it: its helipad, and its airfield's places and runways.
#[derive(Debug, Clone, Default)]
struct ExitPoints {
    helipad: Option<[f32; 2]>,
    airfield: Option<AirfieldBones>,
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
            .filter(|unit| production.fights(unit) && production.armed(unit, combat))
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

/// A train, a build or a research as the mode's data writes it, `[actions.<id>]`: the
/// pipeline's keys it gives, and its kind's section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedAction {
    keys: ActionKeys,
    made: MadeSection,
}

/// The section of what a train, a build or a research makes, of its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MadeSection {
    Train(TrainData),
    Build(BuildData),
    Research(ResearchData),
}

impl ImportedAction {
    /// Its table under `[actions]`.
    pub(crate) fn table(&self) -> TomlTable {
        match &self.made {
            MadeSection::Train(train) => self.keys.table(train),
            MadeSection::Build(build) => self.keys.table(build),
            MadeSection::Research(research) => self.keys.table(research),
        }
    }

    /// The modifier it gives, when it is a research.
    pub(crate) const fn researched(&self) -> Option<&DeclaredName> {
        match &self.made {
            MadeSection::Research(research) => Some(&research.modifier),
            MadeSection::Train(_) | MadeSection::Build(_) => None,
        }
    }
}

/// Why the import drops a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum ButtonDropCause {
    /// No file defines the button, or its object or its upgrade, or an object its prerequisites
    /// name.
    NoObject,
    /// It, or its object's prerequisites, need a science.
    Science,
    /// It trains an object that does not walk, or variations one of which does not.
    NoWalker,
    /// It builds an object of no body, as a small structure (09-zero-hour.md, A cylinder structure
    /// builds as a circle).
    NoBody,
    /// It trains or researches at an object of no `ProductionUpdate`, or of a queue past 1 to
    /// 255.
    NoQueue,
    /// Its object, or an object its prerequisites name, gives no unit type.
    NoType,
    /// Its object's or its upgrade's build time is below 0, or not finite.
    BadTime,
    /// Its action's name, of its kind and its type's or its upgrade's, or its upgrade's own,
    /// passes the bytes a declared name holds.
    LongName,
}

/// A button's kind, of those the import converts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Making {
    Train,
    Build,
}

impl ProductionImport {
    /// A sale's milliseconds, as `BuildAssistant::update` runs one at the game's 30 frames a
    /// second: 45 frames of scaffold, then its construction percent down from 99.9 by 100 over
    /// 90 frames until −50, 135 frames, 180 in all.
    const SELL_MS: u32 = 6_000;

    /// Whether `unit`'s command set holds a gather.
    fn gathers(&self, unit: &DeclaredName) -> bool {
        let commands = self.commands.get(unit);
        commands
            .into_iter()
            .flatten()
            .any(|id| self.supply.gathers.contains_key(id))
    }

    /// Whether a weapon of `combat` arms `unit`, or, for a leader, one of its spawns, which fight
    /// in its place (09-zero-hour.md, A leader of armed spawns is armed).
    fn armed(&self, unit: &DeclaredName, combat: &CombatImport) -> bool {
        let led = self.supply.spawns.get(unit).filter(|spawns| spawns.led);
        combat.arms(unit)
            || led.is_some_and(|spawns| spawns.unit_types.iter().any(|spawn| combat.arms(spawn)))
    }

    /// Whether `unit`'s command set holds trains and researches alone: none of its actions
    /// builds or gathers.
    fn fights(&self, unit: &DeclaredName) -> bool {
        let commands = self.commands.get(unit);
        commands.into_iter().flatten().all(|id| {
            let action = self.actions.get(id);
            action.is_some_and(|action| !matches!(action.made, MadeSection::Build(_)))
        })
    }

    /// The unit types the actions of `kind` of `producer`'s command set make, in its order, but a
    /// jet, made at a place of its airfield, which a base start does not hold.
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
                (MadeSection::Train(train), ActionKind::Train) if !train.at_place => {
                    Some(&train.unit_type)
                }
                (MadeSection::Build(build), ActionKind::Build) => Some(&build.unit_type),
                _ => None,
            })
    }

    /// The trains, builds and supply of `rules`, from the unit types `names` holds, each type a
    /// converted button makes or its variations hold, a building frees, or its object's
    /// prerequisites name, named there, and its own command set's buttons and supply converted in
    /// turn.
    pub(crate) fn of<'r>(
        rules: &'r ZeroHourRules,
        names: &mut ProductionNames<'r>,
        helipads: BTreeMap<DeclaredName, [f32; 2]>,
        airfield_bones: BTreeMap<DeclaredName, AirfieldBones>,
    ) -> Self {
        let mut import = Self {
            helipads,
            airfield_bones,
            ..Self::default()
        };
        let mut queued: VecDeque<NamedObject<'r>> = names.typed_objects().into();
        // A type is visited once, by its name, as several parents may field one variation.
        let mut seen: BTreeSet<TypeName> = queued.iter().map(|named| named.name.clone()).collect();
        while let Some(producer) = queued.pop_front() {
            if let Some(parking) = producer.object.build().parking {
                import.heals.insert(producer.name.clone(), parking.heal);
            }
            let set = producer.object.build().command_set.as_deref();
            let buttons = set.and_then(|set| rules.sets.buttons(set));
            let mut made: Vec<NamedObject<'r>> = Vec::new();
            for (place, button_name) in buttons.into_iter().flatten() {
                let converted =
                    |import: &Self| import.commands.get(&producer.name).map_or(0, Vec::len);
                let before = converted(&import);
                made.extend(import.button(rules, &producer, button_name, names));
                // The action the button made, when it made one its producer did not hold.
                let action = (converted(&import) > before)
                    .then(|| import.commands[&producer.name].last().cloned())
                    .flatten();
                if let Some(button) = rules.buttons.get(button_name) {
                    if button.command == ButtonCommand::Sell {
                        import.sell(rules, &producer);
                    }
                    import.panel.place(
                        &rules.strings,
                        &producer.name,
                        (place, button_name),
                        button,
                        action,
                    );
                }
            }
            made.extend(import.supply_of(rules, &producer, names));
            made.extend(names.take_fresh());
            for named in made {
                if seen.insert(named.name.clone()) {
                    queued.push_back(named);
                }
            }
        }
        import.airfields = import.airfields();
        import
    }

    /// Gives `seller`, whose command set holds a sale, its `sell` section: its `RefundValue` of
    /// money, or else its cost times the game's `SellPercentage`, rounded down, as
    /// `BuildAssistant::update` pays it, over `SELL_MS` (04-capabilities/production.md, A building
    /// may be sold).
    fn sell(&mut self, rules: &ZeroHourRules, seller: &NamedObject<'_>) {
        let build = seller.object.build();
        #[expect(
            clippy::cast_sign_loss,
            reason = "`REAL_TO_UNSIGNEDINT` truncates, and a share below 0 gives back nothing"
        )]
        let refund = if build.refund == 0 {
            (f32::from(build.cost) * rules.game_data.sell_share) as u32
        } else {
            u32::from(build.refund)
        };
        let money = (refund > 0).then(|| (rules.names.money.clone(), refund));
        let sale = SellData {
            refund: money.into_iter().collect(),
            time_ms: Self::SELL_MS,
        };
        self.sells.insert(seller.name.clone(), sale);
    }

    /// Converts the supply of `producer`, a unit type of `names`, its buttons converted: its
    /// node, its drop-off, its gather and its hack, the last of its command slots, and the units it
    /// frees and keeps as its production's, a building that spawns and trains nothing taking the
    /// production of its `ProductionUpdate` (09-zero-hour.md, A spawner takes its production
    /// update); the names of the objects it frees and keeps.
    fn supply_of<'r>(
        &mut self,
        rules: &'r ZeroHourRules,
        named: &NamedObject<'r>,
        names: &mut ProductionNames<'r>,
    ) -> Vec<NamedObject<'r>> {
        self.supply.dock(rules, named);
        let NamedObject {
            name,
            object: producer,
        } = named;
        let gather = self.supply.gather(rules, producer, names);
        let hack = self.supply.hack(rules, producer, names);
        for command in [gather, hack].into_iter().flatten() {
            self.commands.entry(name.clone()).or_default().push(command);
        }
        // A leader keeps its spawns with no production section, as they bud from each other
        // (04-capabilities/production.md, A later spawn buds from the nearest spawn).
        let spawning = if names.imported(name).leads() {
            Spawning::Led
        } else {
            Spawning::ByExit
        };
        let supply = producer.supply();
        let kept = supply.kept().is_some() && spawning == Spawning::ByExit;
        if (kept || supply.one_shots().next().is_some())
            && !self.sections.contains_key(name)
            && let Some(section) = Self::section(producer.build(), self.points(name))
        {
            self.sections.insert(name.clone(), section);
        }
        let section = self.sections.get_mut(name);
        let exits = section.is_some();
        let mut freed = if let Some(section) = section {
            let freed = self.supply.free(rules, producer, names);
            section.free = freed
                .iter()
                .map(|unit| unit.name.declared().clone())
                .collect();
            freed
        } else {
            self.supply.strand(rules, producer, spawning);
            Vec::new()
        };
        if exits || spawning == Spawning::Led {
            freed.extend(self.supply.keep(rules, named, spawning, names));
        }
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
        let points = self.points(&producer.name);
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
            ButtonCommand::PlayerUpgrade | ButtonCommand::ObjectUpgrade => {
                self.research(rules, producer, button_name, button);
                return None;
            }
            // The panel's commands make nothing; the panel places them.
            ButtonCommand::Sell
            | ButtonCommand::Stop
            | ButtonCommand::AttackMove
            | ButtonCommand::SetRallyPoint
            | ButtonCommand::Other => return None,
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
            // A leader walks by its spawns (04-capabilities/production.md, A unit may lead its
            // spawns).
            Making::Train if !imported.walks() && !imported.leads() => {
                return drop(ButtonDropCause::NoWalker);
            }
            Making::Build if !imported.has_body() => return drop(ButtonDropCause::NoBody),
            _ => {}
        }
        let section = match making {
            Making::Train => match Self::section(producer.object.build(), points) {
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
                any.push(typed.name.declared().clone());
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
            .or_insert_with(|| Self::text(&rules.strings, button, &made.name));
        let requires = (!units.is_empty()).then(|| RequiresData {
            units,
            modifiers: Vec::new(),
        });
        let made_section = match making {
            Making::Train => MadeSection::Train(TrainData {
                unit_type: typed.name.declared().clone(),
                requires,
                // A jet is made at a place of its airfield (04-capabilities/production.md, A jet
                // holds a place).
                at_place: typed.imported.sortie().is_some(),
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
            name: typed.name.clone(),
            object: typed.object,
        })
    }

    /// Converts the research button `button_name`, `button`, of the set of `producer`: its action,
    /// made once, of the modifier its upgrade's name gives, for its producer's player or for its
    /// producer as the upgrade's type says, at its cost and after its time, and the producer's
    /// slot, unless it drops the button (04-capabilities/production.md, Research is an action kind
    /// of production).
    fn research(
        &mut self,
        rules: &ZeroHourRules,
        producer: &NamedObject<'_>,
        button_name: &str,
        button: &CommandButton,
    ) {
        let named = button.upgrade.as_deref();
        let upgrade = named.and_then(|name| Some((name, rules.upgrades.get(name)?)));
        let Some((name, upgrade)) = upgrade else {
            return self.dropped.drop(button_name, ButtonDropCause::NoObject);
        };
        if button.science {
            return self.dropped.drop(button_name, ButtonDropCause::Science);
        }
        let points = self.points(&producer.name);
        let Some(section) = Self::section(producer.object.build(), points) else {
            return self.dropped.drop(button_name, ButtonDropCause::NoQueue);
        };
        let Ok(windup_ms) = game_time::of_build_seconds(upgrade.time) else {
            return self.dropped.drop(button_name, ButtonDropCause::BadTime);
        };
        let upgrade_name = ImportName::latin(name);
        let (Ok(id), Ok(modifier)) = (
            upgrade_name.id(ActionKind::Research.name()),
            upgrade_name.declared(),
        ) else {
            return self.dropped.drop(button_name, ButtonDropCause::LongName);
        };
        let message = format!("action-{}", id.as_str().replace('_', "-"));
        self.texts
            .entry(message.clone())
            .or_insert_with(|| Self::text(&rules.strings, button, name));
        let money = rules.names.money.clone();
        let cost = (upgrade.cost > 0).then(|| (money, Self::number(i64::from(upgrade.cost))));
        let windup_ms = i64::try_from(windup_ms).expect("a windup of an `Int` of frames fits");
        let holder = match upgrade.kind {
            UpgradeKind::Player => ResearchHolder::Player,
            UpgradeKind::Object => ResearchHolder::Producer,
        };
        self.actions
            .entry(id.clone())
            .or_insert_with(|| ImportedAction {
                keys: ActionKeys {
                    name: Some(TextId::new(&message).expect("an action's message is a message id")),
                    cost: cost.into_iter().collect(),
                    windup_ms: Some(Self::number(windup_ms)),
                    ..ActionKeys::new(Targeting::None)
                },
                made: MadeSection::Research(ResearchData {
                    modifier,
                    holder,
                    requires: None,
                }),
            });
        let commands = self.commands.entry(producer.name.clone()).or_default();
        if !commands.contains(&id) {
            commands.push(id);
        }
        self.sections.insert(producer.name.clone(), section);
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
    /// none for no `ProductionUpdate`, or a queue past 1 to 255; an airfield of no exit module
    /// makes its units at its `helipad`, with no rally point of its own (09-zero-hour.md, A helipad
    /// is an exit), and its jets at its places, each facing as its bone does, none when one is past
    /// what the engine holds (09-zero-hour.md, The import flies a winged locomotor). The supply's
    /// conversion gives it its free units.
    fn section(build: &IniBuild, points: ExitPoints) -> Option<ProductionData> {
        let queue = u8::try_from(build.queue?).ok().and_then(NonZeroU8::new)?;
        let frame = Self::frame;
        let exit = build.exit;
        let helipad = points
            .helipad
            .filter(|_| build.parking.is_some() && exit.is_none());
        let place = |place: &PlaceBones| {
            Some(PlaceData {
                at: frame(place.at)?,
                angle: game_float::to_num(place.angle).ok()?,
            })
        };
        let parked = (points.airfield).filter(|airfield| {
            build
                .parking
                .is_some_and(|parking| Self::airfield(airfield, parking.heal).is_some())
        });
        let places: Option<Vec<PlaceData>> =
            parked.and_then(|airfield| airfield.places.iter().map(place).collect());
        Some(ProductionData {
            queue,
            exit: match helipad {
                Some(helipad) => frame(helipad),
                None => exit.and_then(|exit| frame(exit.create)),
            },
            rally: exit.and_then(|exit| frame(exit.rally)),
            gathers: exit.is_some_and(|exit| exit.starts_gatherers),
            free: Vec::new(),
            places: places.unwrap_or_default(),
        })
    }

    /// The helipad and the places of the airfield `name`, none for another.
    fn points(&self, name: &TypeName) -> ExitPoints {
        ExitPoints {
            helipad: self.helipads.get(name.declared()).copied(),
            airfield: self.airfield_bones.get(name.declared()).cloned(),
        }
    }

    /// The point `point` of an object's frame, the game's axes, in the engine's, none past what the
    /// engine holds.
    fn frame(point: [f32; 2]) -> Option<FramePoint> {
        let [x, y] = point.map(|axis| game_float::to_num(axis).ok());
        FramePoint::new(axes::ground([x?, y?])?)
    }

    /// The `airfield` of `bones` and of `heal`, its `HealAmountPerSecond`: each place's prep point
    /// and runway, each runway's start and end, in the engine's frame, and its heal, none of 0 or
    /// less; none when a point or the heal is past what the engine holds, so its production makes
    /// no unit at a place either.
    fn airfield(bones: &AirfieldBones, heal: f32) -> Option<AirfieldData> {
        let places = bones.places.iter().map(|place| {
            Some(AirfieldPlace {
                prep: Self::frame(place.prep)?,
                runway: place.runway,
            })
        });
        let runways = bones.runways.iter().map(|&[start, end]| {
            Some(RunwayData {
                start: Self::frame(start)?,
                end: Self::frame(end)?,
            })
        });
        let heal = match game_float::to_num(heal).ok()? {
            heal if heal > Num::ZERO => Some(heal),
            _ => None,
        };
        Some(AirfieldData {
            places: places.collect::<Option<_>>()?,
            runways: runways.collect::<Option<_>>()?,
            heal,
        })
    }

    /// The `airfield` of each producer whose production makes units at places, of its bones.
    fn airfields(&self) -> BTreeMap<TypeName, AirfieldData> {
        let parks = self
            .sections
            .iter()
            .filter(|(_, data)| !data.places.is_empty());
        parks
            .filter_map(|(name, _)| {
                let bones = self.airfield_bones.get(name.declared())?;
                let heal = self.heals.get(name).copied().unwrap_or_default();
                Some((name.clone(), Self::airfield(bones, heal)?))
            })
            .collect()
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

    /// The name of the action of `button`, which makes or researches `made`: the text of its
    /// label, its hotkey's marker dropped, `&&` an `&`; `made`'s name when the table lacks the
    /// label.
    fn text(strings: &StringTable, button: &CommandButton, made: &str) -> String {
        let Some(text) = button.label.as_deref().and_then(|label| strings.get(label)) else {
            return made.to_owned();
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
