use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::num::NonZeroU8;

use campfire_actions::{ActionKeys, ActionKind, ActionRange, RangeField, Targeting};
use campfire_airfields::{AirfieldData, AirfieldPlace, ApproachData, RunwayData};
use campfire_codec::TomlTable;
use campfire_common::DeclaredName;
use campfire_math::{Num, Rounding};
use campfire_production::{
    BuildData, BuildStart, ConstructData, DoorsData, FramePoint, PlaceData, ProductionData,
    RequiredUnit, RequiresData, ResearchData, ResearchHolder, SellData, TrainData,
};
use campfire_values::{FilterData, Number, Ranked, Share, Stat, TextId};

use crate::zero_hour::additions::{Additions, TypeAdditions};
use crate::zero_hour::airfield_bones::{AirfieldBones, PlaceBones};
use crate::zero_hour::area_write::AreaWrite;
use crate::zero_hour::axes;
use crate::zero_hour::button_drop_cause::ButtonDropCause;
use crate::zero_hour::dropped::Losses;
use crate::zero_hour::error::NameError;
use crate::zero_hour::format::csf::string_table::StringTable;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::command_button_ini::{ButtonCommand, CommandButton};
use crate::zero_hour::format::ini::creation_list_ini::CreationNugget;
use crate::zero_hour::format::ini::object_ini::ini_build::{IniBuild, IniParking, Prerequisite};
use crate::zero_hour::format::ini::object_ini::ini_object::IniObject;
use crate::zero_hour::format::ini::object_ini::ini_power::IniAutoHeal;
use crate::zero_hour::format::ini::upgrade_ini::UpgradeKind;
use crate::zero_hour::game_time;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::NamedObject;
use crate::zero_hour::imported_type::ImportedType;
use crate::zero_hour::panel_import::PanelButtons;
use crate::zero_hour::power_drop_cause::PowerDropCause;
use crate::zero_hour::production_bones::ProductionBones;
use crate::zero_hour::production_cx::ProductionCx;
use crate::zero_hour::production_import::power_cast::{PowerCast, PowerModifier, PowerNames};
use crate::zero_hour::supply_import::{DOCK_RANGE, Spawning, SupplyImport};
use crate::zero_hour::type_name::TypeName;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// Zero Hour's trains, builds and researches (09-zero-hour.md, Every command set's trains and
/// builds are imported; 04-capabilities/production.md, Research is an action kind of production):
/// each `UNIT_BUILD`, `DOZER_CONSTRUCT`, `PLAYER_UPGRADE` and `OBJECT_UPGRADE` button of the
/// command set of each unit type, and of each type a converted button makes, once for the whole
/// import; the names of their actions, as the string table gives their labels; the buttons dropped,
/// each once for each cause, in the order the sets are read; and the supply of each type it
/// visits. It gives each producer its actions of the slot kind `command`, its `production` and
/// its `sell`, each airfield its `airfield`, and each marker its area, as additions.
#[derive(Debug, Default)]
pub(crate) struct ProductionImport {
    pub(crate) actions: BTreeMap<DeclaredName, ImportedAction>,
    pub(crate) texts: BTreeMap<String, String>,
    pub(crate) dropped: Losses<ButtonDropCause>,
    /// The docks, supply centers, gatherers and free units of the types it visits.
    pub(crate) supply: SupplyImport,
    /// The places of each producer's buttons on its panel, and their faces.
    pub(crate) panel: PanelButtons,
    /// What it reads of the models: the helipad an airfield's trains leave from, each airfield's
    /// places and runways, which its jets take, and each dock's point.
    bones: ProductionBones,
    /// The `ParkingPlaceBehavior` of each producer of one it visits.
    parkings: BTreeMap<TypeName, IniParking>,
    /// The sciences its trains, builds and researches need, by their names in the game, whose perks
    /// give their modifiers (04-capabilities/progression.md, A perk's modifier is a player
    /// modifier).
    pub(crate) sciences: BTreeSet<String>,
    /// The powers it converts to casts: what each perk's modifier gives for them, by the perk's
    /// name, the stats of their heals, and the powers' buttons dropped.
    pub(crate) power_modifiers: BTreeMap<DeclaredName, PowerModifier>,
    pub(crate) power_stats: BTreeSet<Stat>,
    pub(crate) powers_dropped: Losses<PowerDropCause>,
}

/// A button of a command set, by its name in `CommandButton.ini`.
#[derive(Debug, Clone, Copy)]
struct NamedButton<'b> {
    name: &'b str,
    button: &'b CommandButton,
}

/// Where a producer's units leave it: its helipad, and its airfield's places and runways.
#[derive(Debug, Clone, Default)]
struct ExitPoints {
    helipad: Option<[f32; 2]>,
    airfield: Option<AirfieldBones>,
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
    Cast(PowerCast),
}

impl ImportedAction {
    /// Its table under `[actions]`.
    pub(crate) fn table(&self) -> TomlTable {
        match &self.made {
            MadeSection::Train(train) => self.keys.table(train),
            MadeSection::Build(build) => self.keys.table(build),
            MadeSection::Research(research) => self.keys.table(research),
            MadeSection::Cast(cast) => cast.table(&self.keys),
        }
    }

    /// The modifier it gives, when it is a research.
    pub(crate) const fn researched(&self) -> Option<&DeclaredName> {
        match &self.made {
            MadeSection::Research(research) => Some(&research.modifier),
            MadeSection::Train(_) | MadeSection::Build(_) | MadeSection::Cast(_) => None,
        }
    }
}

/// What a train's or a build's prerequisites require: the objects of each line of objects, any one
/// of which passes it, and the sciences of its lines of a science.
#[derive(Debug, Default)]
struct Required<'a> {
    units: Vec<Vec<&'a IniObject>>,
    sciences: Vec<&'a str>,
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

    /// How far past a runway's end its approach lies, as a share of the runway's length, as
    /// `ParkingPlaceBehavior::calcPPInfo`'s `APPROACH_DIST` places it.
    const APPROACH_PAST: Num = Num::from_bits(3 << (Num::FRAC_BITS - 2));

    /// The trains, builds and supply of `cx`'s rules, from the unit types its names hold, each type
    /// a converted button makes or its variations hold, a building frees, or its object's
    /// prerequisites name, named there, and its own command set's buttons and supply converted in
    /// turn, of the points `bones` gives.
    pub(crate) fn of<'r>(mut cx: ProductionCx<'r, '_>, bones: ProductionBones) -> Self {
        let rules = cx.rules;
        let mut import = Self {
            bones,
            ..Self::default()
        };
        let mut queued: VecDeque<NamedObject<'r>> = cx.names.typed_objects().into();
        // A type is visited once, by its name, as several parents may field one variation.
        let mut seen: BTreeSet<TypeName> = queued.iter().map(|named| named.name.clone()).collect();
        while let Some(producer) = queued.pop_front() {
            if let Some(parking) = producer.object.build().parking {
                import.parkings.insert(producer.name.clone(), parking);
            }
            let set = producer.object.build().command_set.as_deref();
            let buttons = set.and_then(|set| rules.sets.buttons(set));
            let mut made: Vec<NamedObject<'r>> = Vec::new();
            for (place, button_name) in buttons.into_iter().flatten() {
                let before = cx.additions.commands(&producer.name).len();
                made.extend(import.button(&mut cx, &producer, button_name));
                // The action the button made, when it made one its producer did not hold.
                let after = cx.additions.commands(&producer.name);
                let action = (after.len() > before)
                    .then(|| after.last().cloned())
                    .flatten();
                if let Some(button) = rules.buttons.get(button_name) {
                    if button.command == ButtonCommand::Sell {
                        import.sell(&mut cx, &producer);
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
            made.extend(import.supply_of(&mut cx, &producer));
            made.extend(cx.names.take_fresh());
            for named in made {
                if seen.insert(named.name.clone()) {
                    queued.push_back(named);
                }
            }
        }
        import.airfields(cx.additions);
        import
    }

    /// Gives `seller`, whose command set holds a sale, its `sell` section: its `RefundValue` of
    /// money, or else its cost times the game's `SellPercentage`, rounded down, as
    /// `BuildAssistant::update` pays it, over `SELL_MS` (04-capabilities/production.md, A building
    /// may be sold).
    fn sell(&self, cx: &mut ProductionCx<'_, '_>, seller: &NamedObject<'_>) {
        let rules = cx.rules;
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
        cx.additions.type_mut(&seller.name).sell = Some(sale);
    }

    /// Converts the supply of `producer`, a unit type of `names`, its buttons converted: its
    /// node, its drop-off, its gather and its hack, the last of its command slots, and the units it
    /// frees and keeps as its production's, a building that spawns and trains nothing taking the
    /// production of its `ProductionUpdate` (09-zero-hour.md, A spawner takes its production
    /// update); the names of the objects it frees and keeps.
    fn supply_of<'r>(
        &mut self,
        cx: &mut ProductionCx<'r, '_>,
        named: &NamedObject<'r>,
    ) -> Vec<NamedObject<'r>> {
        let rules = cx.rules;
        let dock = self.bones.docks.get(named.name.declared()).copied();
        self.supply.dock(cx, named, dock.and_then(Self::frame));
        let NamedObject {
            name,
            object: producer,
        } = named;
        let gather = self.supply.gather(rules, producer, cx.names);
        let hack = self.supply.hack(rules, producer, cx.names);
        for command in [gather, hack].into_iter().flatten() {
            cx.additions.type_mut(name).commands.push(command);
        }
        // A leader keeps its spawns with no production section, as they bud from each other
        // (04-capabilities/production.md, A later spawn buds from the nearest spawn).
        let spawning = if cx.names.imported(name).leads() {
            Spawning::Led
        } else {
            Spawning::ByExit
        };
        let supply = producer.supply();
        let kept = supply.kept().is_some() && spawning == Spawning::ByExit;
        let produces = |additions: &Additions| {
            let added = additions.get(name);
            added.is_some_and(|added| added.production.is_some())
        };
        if (kept || supply.one_shots().next().is_some())
            && !produces(cx.additions)
            && let Some(section) = Self::section(producer.build(), self.points(name))
        {
            cx.additions.type_mut(name).production = Some(section);
        }
        let exits = produces(cx.additions);
        let mut freed = if exits {
            let freed = self.supply.free(rules, producer, cx.names);
            let section = cx.additions.type_mut(name).production.as_mut();
            section.expect("a producer exits by its section").free = freed
                .iter()
                .map(|unit| unit.name.declared().clone())
                .collect();
            freed
        } else {
            self.supply.strand(rules, producer, spawning);
            Vec::new()
        };
        if exits || spawning == Spawning::Led {
            freed.extend(self.supply.keep(cx, named, spawning));
        }
        freed
    }

    /// Converts the button `button_name` of the set of `producer`: its action, made once, and
    /// the producer's slot; the name of the object it makes, unless it drops the button or the
    /// button is no train or build.
    fn button<'r>(
        &mut self,
        cx: &mut ProductionCx<'r, '_>,
        producer: &NamedObject<'r>,
        button_name: &str,
    ) -> Option<NamedObject<'r>> {
        let rules = cx.rules;
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
                let named = NamedButton {
                    name: button_name,
                    button,
                };
                self.research(cx, producer, named);
                return None;
            }
            ButtonCommand::SpecialPower => {
                let named = NamedButton {
                    name: button_name,
                    button,
                };
                return self.power(cx, producer, named);
            }
            // The panel's commands make nothing; the panel places them, and drops a purchase.
            ButtonCommand::Sell
            | ButtonCommand::Stop
            | ButtonCommand::AttackMove
            | ButtonCommand::SetRallyPoint
            | ButtonCommand::ExitContainer
            | ButtonCommand::Evacuate
            | ButtonCommand::PurchaseScience
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
        let Required {
            units: requires,
            sciences,
        } = match Self::requires(rules, build) {
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
                let Ok(typed) = cx.names.unit_type(required, rules) else {
                    return drop(ButtonDropCause::NoType);
                };
                any.push(typed.name.declared().clone());
            }
            units.push(RequiredUnit::any_of(any).expect("a prerequisite line names an object"));
        }
        let Ok(typed) = cx.names.unit_type(made, rules) else {
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
        let Ok(modifiers) = Self::science_modifiers(&mut self.sciences, sciences.into_iter())
        else {
            return drop(ButtonDropCause::LongName);
        };
        let requires = (!units.is_empty() || !modifiers.is_empty())
            .then_some(RequiresData { units, modifiers });
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
        let added = cx.additions.type_mut(&producer.name);
        Self::command(added, id);
        if let Some(section) = section {
            added.production = Some(section);
        }
        Some(NamedObject {
            name: typed.name.clone(),
            object: typed.object,
        })
    }

    /// Converts the special power button `named` of the set of `producer` to a
    /// cast of its producer, made once (09-zero-hour.md, A power is a cast of its building): of an
    /// `OCLSpecialPower` of its power made where its player aims, each of whose lists makes one
    /// object of a single-burst `AutoHealBehavior` of vehicles, the first list's object the area
    /// it lands, and of the power's science, whose perk's modifier gives the producer the cast's
    /// tag and the heal, each `UpgradeOCL` science's what its list heals past the one below; the
    /// marker's object, which the cast's area is, unless it drops the button.
    fn power<'r>(
        &mut self,
        cx: &mut ProductionCx<'r, '_>,
        producer: &NamedObject<'r>,
        named: NamedButton<'_>,
    ) -> Option<NamedObject<'r>> {
        let (rules, button) = (cx.rules, named.button);
        let mut drop = |cause| {
            self.powers_dropped.drop(named.name, cause);
            None
        };
        let power = button.power.as_deref();
        let held = power.and_then(|name| Some((name, rules.powers.named(name)?)));
        let Some((name, power)) = held else {
            return drop(PowerDropCause::NoPower);
        };
        let module = producer.object.powers().ocl.iter();
        let Some(module) = module
            .into_iter()
            .find(|module| module.power.as_deref() == Some(name))
        else {
            return drop(PowerDropCause::NoPower);
        };
        if !module.at_location {
            return drop(PowerDropCause::NotAtLocation);
        }
        // The one object a list makes, of a single burst that starts on.
        let marker = |list: &str| {
            let nuggets = rules.creation_lists.lists().get(list)?;
            let [CreationNugget::Objects(creation)] = &**nuggets else {
                return None;
            };
            let [object] = &*creation.names else {
                return None;
            };
            let object = rules.objects.object(object.as_bytes())?;
            let heal = object.powers().heal.as_ref()?;
            (creation.count == 1 && heal.single_burst && heal.starts_active)
                .then_some((object, heal))
        };
        let Some((object, first)) = module.list.as_deref().and_then(marker) else {
            return drop(PowerDropCause::NoBurst);
        };
        let upgrades = module
            .upgrades
            .iter()
            .map(|upgrade| marker(&upgrade.list).map(|(_, heal)| (upgrade.science.as_str(), heal)));
        let Some(upgrades) = upgrades.collect::<Option<Vec<_>>>() else {
            return drop(PowerDropCause::NoBurst);
        };
        let vehicles = |heal: &IniAutoHeal| matches!(&*heal.kinds, [kind] if kind.eq_ignore_ascii_case("vehicle"));
        if !vehicles(first) || !upgrades.iter().all(|(_, heal)| vehicles(heal)) {
            return drop(PowerDropCause::Kind);
        }
        let Some(science) = power.science.as_deref() else {
            return drop(PowerDropCause::NoScience);
        };
        let perk = |science: &str| ImportName::latin(science).declared();
        let named = PowerNames::of(name).and_then(|names| {
            let first = perk(science)?;
            let upgrades = upgrades
                .iter()
                .map(|&(science, heal)| Ok((perk(science)?, heal)))
                .collect::<Result<Vec<_>, NameError>>()?;
            Ok((names, first, upgrades))
        });
        let Ok((power_names, first_perk, upgrades)) = named else {
            return drop(PowerDropCause::LongName);
        };
        let Ok(radius) = game_float::to_num(first.radius) else {
            return drop(PowerDropCause::NoBurst);
        };
        let Ok(typed) = cx.names.unit_type(object, rules) else {
            return drop(PowerDropCause::NoBurst);
        };
        self.sciences.insert(science.to_owned());
        self.sciences.extend(
            module
                .upgrades
                .iter()
                .map(|upgrade| upgrade.science.clone()),
        );
        let id = power_names.id.clone();
        let message = format!("action-{}", id.as_str().replace('_', "-"));
        self.texts
            .entry(message.clone())
            .or_insert_with(|| Self::text(&rules.strings, button, name));
        let area = typed.name.declared().clone();
        cx.additions.type_mut(typed.name).area = Some(AreaWrite {
            radius,
            affects: FilterData::parse("allies:vehicle").expect("a filter the import gives"),
        });
        self.power_modifiers.extend(PowerModifier::of(
            &power_names,
            (first_perk, first),
            &upgrades,
        ));
        self.power_stats.insert(power_names.heal.clone());
        self.actions
            .entry(id.clone())
            .or_insert_with(|| ImportedAction {
                keys: PowerCast::keys(&power_names, power.reload_ms, &message),
                made: MadeSection::Cast(PowerCast::new(area)),
            });
        Self::command(cx.additions.type_mut(&producer.name), id);
        Some(NamedObject {
            name: typed.name.clone(),
            object: typed.object,
        })
    }

    /// Converts the research button `named` of the set of `producer`: its action,
    /// made once, of the modifier its upgrade's name gives, for its producer's player or for its
    /// producer as the upgrade's type says, at its cost and after its time, and the producer's
    /// slot, unless it drops the button (04-capabilities/production.md, Research is an action kind
    /// of production).
    fn research(
        &mut self,
        cx: &mut ProductionCx<'_, '_>,
        producer: &NamedObject<'_>,
        named: NamedButton<'_>,
    ) {
        let (rules, button_name, button) = (cx.rules, named.name, named.button);
        let named = button.upgrade.as_deref();
        let upgrade = named.and_then(|name| Some((name, rules.upgrades.get(name)?)));
        let Some((name, upgrade)) = upgrade else {
            return self.dropped.drop(button_name, ButtonDropCause::NoObject);
        };
        let sciences = button.sciences.iter().map(String::as_str);
        let Ok(modifiers) = Self::science_modifiers(&mut self.sciences, sciences) else {
            return self.dropped.drop(button_name, ButtonDropCause::LongName);
        };
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
                    requires: (!modifiers.is_empty()).then(|| RequiresData {
                        units: Vec::new(),
                        modifiers,
                    }),
                }),
            });
        let added = cx.additions.type_mut(&producer.name);
        Self::command(added, id);
        added.production = Some(section);
    }

    /// Gives `added`, a producer's, the action `id` of its slot kind `command`, once.
    fn command(added: &mut TypeAdditions, id: DeclaredName) {
        if !added.commands.contains(&id) {
            added.commands.push(id);
        }
    }

    /// The objects `build`'s prerequisites require, each line the objects any one of which passes
    /// it, as `ProductionPrerequisite::isSatisfied` reads a line (09-zero-hour.md, A prerequisite
    /// line converts to a list), and the sciences its lines of a science need; a cause for a line
    /// of an object no file defines.
    fn requires<'a>(
        rules: &'a ZeroHourRules,
        build: &'a IniBuild,
    ) -> Result<Required<'a>, ButtonDropCause> {
        let mut required = Required::default();
        for line in &build.prerequisites {
            match line {
                Prerequisite::Science(science) => required.sciences.push(science),
                Prerequisite::Objects(objects) => {
                    let objects = objects.iter().map(|object| {
                        let object = rules.objects.object(object.as_bytes());
                        object.ok_or(ButtonDropCause::NoObject)
                    });
                    required.units.push(objects.collect::<Result<_, _>>()?);
                }
            }
        }
        Ok(required)
    }

    /// The modifiers of the perks of `sciences`, each of which `kept` keeps; an error for a name
    /// past the most a name holds.
    fn science_modifiers<'s>(
        kept: &mut BTreeSet<String>,
        sciences: impl Iterator<Item = &'s str>,
    ) -> Result<Vec<DeclaredName>, NameError> {
        sciences
            .map(|science| {
                kept.insert(science.to_owned());
                ImportName::latin(science).declared()
            })
            .collect()
    }

    /// The `production` section of a producer of `build`: its queue, its exit's points in the
    /// engine's frame, the game's `y` its `−z`, as a map's places turn, each none for a number the
    /// engine does not hold or a point past its bound, and whether its exit starts its gatherers;
    /// none for no `ProductionUpdate`, or a queue past 1 to 255; an airfield of no exit module
    /// makes its units at its `helipad`, with no rally point of its own (09-zero-hour.md, A helipad
    /// is an exit), and its jets at its places, each facing as its bone does and made in its
    /// hangar, none when one is past what the engine holds (09-zero-hour.md, The import flies a
    /// winged locomotor; 04-capabilities/production.md, A hangar is a jet's start), and its doors,
    /// of a module of door animations (04-capabilities/production.md, A producer's door opens
    /// before its unit leaves). The supply's conversion gives it its free units.
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
                exit: match place.hangar {
                    Some(hangar) => Some(frame(hangar)?),
                    None => None,
                },
            })
        };
        let parked = (points.airfield).filter(|airfield| {
            build
                .parking
                .is_some_and(|parking| Self::airfield(airfield, parking).is_some())
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
            doors: NonZeroU8::new(build.doors.count).map(|count| DoorsData {
                count,
                opening_ms: build.doors.opening_ms,
                open_ms: build.doors.open_ms,
                closing_ms: build.doors.closing_ms,
            }),
        })
    }

    /// The helipad and the places of the airfield `name`, none for another.
    fn points(&self, name: &TypeName) -> ExitPoints {
        ExitPoints {
            helipad: self.bones.helipads.get(name.declared()).copied(),
            airfield: self.bones.airfields.get(name.declared()).cloned(),
        }
    }

    /// The point `point` of an object's frame, the game's axes, in the engine's, none past what the
    /// engine holds.
    fn frame(point: [f32; 2]) -> Option<FramePoint> {
        let [x, y] = point.map(|axis| game_float::to_num(axis).ok());
        FramePoint::new(axes::ground([x?, y?])?)
    }

    /// The `airfield` of `bones` and of `parking`: each place's prep point and runway, each
    /// runway's start and end, in the engine's frame, its approach at its `ApproachHeight`, past
    /// each runway's end by `APPROACH_PAST` of its length, and its heal, its `HealAmountPerSecond`,
    /// none of 0 or less; none when a point, the height or the heal is past what the engine holds,
    /// so its production makes no unit at a place either.
    fn airfield(bones: &AirfieldBones, parking: IniParking) -> Option<AirfieldData> {
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
        let heal = match game_float::to_num(parking.heal).ok()? {
            heal if heal > Num::ZERO => Some(heal),
            _ => None,
        };
        Some(AirfieldData {
            places: places.collect::<Option<_>>()?,
            runways: runways.collect::<Option<_>>()?,
            approach: ApproachData {
                height: game_float::to_num(parking.approach_height).ok()?,
                past: Self::APPROACH_PAST,
            },
            heal,
        })
    }

    /// Gives each producer of `additions` whose production makes units at places its `airfield`
    /// (09-zero-hour.md, The import flies a winged locomotor), of its bones.
    fn airfields(&self, additions: &mut Additions) {
        for (name, added) in additions.iter_mut() {
            let parks = added.production.as_ref();
            if parks.is_none_or(|data| data.places.is_empty()) {
                continue;
            }
            let Some(bones) = self.bones.airfields.get(name.declared()) else {
                continue;
            };
            let parking = self.parkings.get(name).copied().unwrap_or_default();
            added.airfield = Self::airfield(bones, parking);
        }
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

pub(crate) mod faction_base;
pub(crate) mod power_cast;

#[cfg(test)]
mod tests;
