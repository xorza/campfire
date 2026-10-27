use std::collections::BTreeMap;
use std::num::{NonZeroU8, NonZeroU32};

use campfire_actions::{
    ActionKeys, ActionKind, ActionRange, ChannelData, ChannelPaysData, RangeField, Targeting,
};
use campfire_codec::TomlTable;
use campfire_common::{DeclaredName, EnumAll};
use campfire_math::Num;
use campfire_production::{DropOffData, EmptyNode, GatherData, NodeData, SpawnSpec, SpawnsData};
use campfire_values::{Distance, Number, Ranked, Scalar};
use serde::{Deserialize, Serialize};

use crate::zero_hour::dropped::Losses;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::object_ini::IniObject;
use crate::zero_hour::format::ini::object_ini::ini_supply::{
    InternetHack, SupplyDock, SupplyGatherer,
};
use crate::zero_hour::game_time;
use crate::zero_hour::import_names::{NamedObject, ProductionNames};
use crate::zero_hour::imported_type::ImportedType;
use crate::zero_hour::type_name::TypeName;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// Zero Hour's supply, of the unit types production visits (09-zero-hour.md, A box is
/// money): each dock a node of money, its boxes' worth; each supply center a drop-off of money;
/// each gatherer that walks a gather of its own numbers; the free units of each producer, its
/// one-shot spawns that walk; each hacker's hack; and what it drops, each once for each cause, in
/// the order the types are visited.
#[derive(Debug, Default)]
pub(crate) struct SupplyImport {
    pub(crate) gathers: BTreeMap<DeclaredName, ImportedGather>,
    pub(crate) hacks: BTreeMap<DeclaredName, ActionKeys>,
    pub(crate) nodes: BTreeMap<TypeName, NodeData>,
    pub(crate) drop_offs: BTreeMap<TypeName, DropOffData>,
    /// The spawns each building of a production section keeps, by its type's name.
    pub(crate) spawns: BTreeMap<TypeName, SpawnsData>,
    pub(crate) dropped: Losses<SupplyDropCause>,
}

/// A gather as the mode's data writes it, `[actions.<id>]`: the pipeline's keys it gives, and its
/// kind's section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedGather {
    keys: ActionKeys,
    gather: GatherData,
}

/// Why the import drops a supply module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum SupplyDropCause {
    /// A dock that holds no box.
    NoBoxes,
    /// A number past what the engine holds.
    BadNumber,
    /// A gatherer, or a unit a building spawns, that does not move.
    NoWalker,
    /// A unit a building of no production spawns, which has no exit to leave.
    NoExit,
    /// A spawned unit that no file defines.
    NoObject,
    /// A kept spawn of no life, which never dies to come back.
    Unliving,
    /// A gatherer whose gather's name, of its kind and its type's, passes the bytes a declared
    /// name holds.
    LongName,
    /// An `OCLUpdate`, which creates objects over time, as America's supply drop zone sends a
    /// cargo plane of crates (09-zero-hour.md, A supply drop zone's crates are lost).
    ObjectsOverTime,
    /// A hack's cash at a veterancy past regular that is not its regular cash, as the import
    /// gives no unit a veterancy.
    HackVeterancy,
    /// A hack's `XpPerCashUpdate`, as the import gives no unit a veterancy.
    HackExperience,
    /// A hack's `CashUpdateDelayFast`, its delay in a building, as the import puts no unit in
    /// one.
    HackInBuilding,
    /// A hack's pack spread by its `PackUnpackVariationFactor`, as a channel's wind-down does not
    /// spread (04-capabilities/actions.md, A channel with no end).
    PackSpread,
    /// A leader's spawns, which take its orders in place of their own `MobMemberSlavedUpdate`:
    /// its catch-up, its crisis bail and its self-tasking go, and no spawn of a hurt leader goes
    /// prone (09-zero-hour.md, The import converts a mob).
    LedSpawns,
    /// A spawn whose object gives no unit type, as one of a locomotor no file defines, which the
    /// import refuses as it refuses the type.
    NoType,
}

/// How a spawner sends its kept spawns: each out of its exit, or as their leader, which buds each
/// later one from the nearest (04-capabilities/production.md, A later spawn buds from the nearest
/// spawn).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Spawning {
    ByExit,
    Led,
}

/// How near a builder comes to its building's box, and a gatherer to its dock's, 10 m, the
/// pathfinder's cell: the dozer docks in the free cell beside the building nearest it
/// (09-zero-hour.md, Every command set's trains and builds are imported; 09-zero-hour.md, A
/// gatherer docks as a dozer builds).
pub(crate) const DOCK_RANGE: Num = Num::int(10);

impl SupplyImport {
    /// Adds the node of `producer` when it is a dock, and its drop-off when it is a supply
    /// center, by the box value of `rules`; drops what it creates over time.
    pub(crate) fn dock(&mut self, rules: &ZeroHourRules, producer: &NamedObject<'_>) {
        let object = producer.object;
        let supply = object.supply();
        if supply.creates_over_time {
            self.drop(object, SupplyDropCause::ObjectsOverTime);
        }
        if supply.center {
            let money = rules.names.money.clone();
            let drop_off = DropOffData {
                resources: vec![money],
            };
            self.drop_offs.insert(producer.name.clone(), drop_off);
        }
        let Some(dock) = supply.dock else {
            return;
        };
        match Self::node(dock, rules) {
            Ok(node) => {
                self.nodes.insert(producer.name.clone(), node);
            }
            Err(cause) => self.drop(object, cause),
        }
    }

    /// The gather of `object`, a unit type of `names`, when it is a gatherer that walks, by the
    /// box value of `rules`: its action's name, which goes in its command slots. An AI module of
    /// no box, as a Helix's `ChinookAIUpdate`, gathers nothing in the original either, so it is no
    /// gatherer.
    pub(crate) fn gather<'r>(
        &mut self,
        rules: &'r ZeroHourRules,
        object: &'r IniObject,
        names: &mut ProductionNames<'r>,
    ) -> Option<DeclaredName> {
        let gatherer = object.supply().gatherer?;
        let boxes = u32::try_from(gatherer.max_boxes)
            .ok()
            .and_then(NonZeroU32::new)?;
        let typed = names.unit_type(object, rules).ok()?;
        if !typed.imported.walks() {
            self.drop(object, SupplyDropCause::NoWalker);
            return None;
        }
        let imported = match Self::imported(gatherer, boxes, rules) {
            Ok(imported) => imported,
            Err(cause) => {
                self.drop(object, cause);
                return None;
            }
        };
        let id = typed.name.id(ActionKind::Gather.name());
        let Ok(id) = id else {
            self.drop(object, SupplyDropCause::LongName);
            return None;
        };
        self.gathers.entry(id.clone()).or_insert(imported);
        Some(id)
    }

    /// The hack of `object`, a unit type of `names`, when it is a hacker that walks: its action's
    /// name, which goes in its command slots; its parts the engine does not hold are dropped
    /// (04-capabilities/actions.md, A channel with no end).
    pub(crate) fn hack<'r>(
        &mut self,
        rules: &'r ZeroHourRules,
        object: &'r IniObject,
        names: &mut ProductionNames<'r>,
    ) -> Option<DeclaredName> {
        let hacker = object.supply().hacker?;
        let typed = names.unit_type(object, rules).ok()?;
        if !typed.imported.walks() {
            self.drop(object, SupplyDropCause::NoWalker);
            return None;
        }
        let keys = match Self::hacking(hacker, rules) {
            Ok(keys) => keys,
            Err(cause) => {
                self.drop(object, cause);
                return None;
            }
        };
        let Ok(id) = typed.name.id("hack") else {
            self.drop(object, SupplyDropCause::LongName);
            return None;
        };
        let cash = Self::cash(hacker.cash);
        let lost = [
            (
                cash.iter().any(|&amount| amount != cash[0]),
                SupplyDropCause::HackVeterancy,
            ),
            (hacker.xp_per_cash > 0, SupplyDropCause::HackExperience),
            (
                game_time::of_ms(hacker.cash_delay_fast_ms).ok()
                    != game_time::of_ms(hacker.cash_delay_ms).ok(),
                SupplyDropCause::HackInBuilding,
            ),
            (
                hacker.variation != 0.0 && hacker.pack_ms > 0,
                SupplyDropCause::PackSpread,
            ),
        ];
        for (_, cause) in lost.into_iter().filter(|(lost, _)| *lost) {
            self.drop(object, cause);
        }
        self.hacks.entry(id.clone()).or_insert(keys);
        Some(id)
    }

    /// The units `object`, a building that trains, frees: its one-shot spawns that walk, each
    /// typed in `names`, which production visits in turn.
    pub(crate) fn free<'r>(
        &mut self,
        rules: &'r ZeroHourRules,
        object: &'r IniObject,
        names: &mut ProductionNames<'r>,
    ) -> Vec<NamedObject<'r>> {
        let mut freed = Vec::new();
        for template in object.supply().one_shots() {
            let Some(spawned) = self.walker(rules, template) else {
                continue;
            };
            match names.unit_type(spawned, rules) {
                Ok(typed) => freed.push(NamedObject {
                    name: typed.name.clone(),
                    object: typed.object,
                }),
                Err(_) => self.drop(spawned, SupplyDropCause::NoType),
            }
        }
        freed
    }

    /// Keeps the spawns of `producer`, a building that trains or, by `spawning`, their leader:
    /// its kept spawn's templates in turn, each a walker typed in `names`, its number and its
    /// replace delay as the import converts a time (09-zero-hour.md, A kept `SpawnBehavior`
    /// converts to spawns; 04-capabilities/production.md, Spawns take their types in turn), led
    /// when it leads them (09-zero-hour.md, The import converts a mob); the objects of its spawns'
    /// types, to join the walk. None for no kept spawn, and its spawn dropped for a number or a
    /// delay past what the engine holds, more templates than a spawner takes, or a spawn of no walk
    /// or no life.
    pub(crate) fn keep<'r>(
        &mut self,
        rules: &'r ZeroHourRules,
        producer: &NamedObject<'r>,
        spawning: Spawning,
        names: &mut ProductionNames<'r>,
    ) -> Vec<NamedObject<'r>> {
        let Some(kept) = producer.object.supply().kept() else {
            return Vec::new();
        };
        if kept.templates.len() > SpawnSpec::MOST_TYPES {
            self.drop_named(&kept.templates[0], SupplyDropCause::BadNumber);
            return Vec::new();
        }
        let mut walkers = Vec::with_capacity(kept.templates.len());
        for template in &kept.templates {
            let Some(spawned) = self.walker(rules, template) else {
                return Vec::new();
            };
            walkers.push(spawned);
        }
        let count = u8::try_from(kept.number).ok().and_then(NonZeroU8::new);
        let replace_ms = game_time::of_ms(kept.replace_ms).ok();
        let replace_ms = replace_ms.and_then(|ms| u32::try_from(ms).ok());
        let (Some(count), Some(replace_ms)) = (count, replace_ms) else {
            self.drop(walkers[0], SupplyDropCause::BadNumber);
            return Vec::new();
        };
        let mut units: Vec<NamedObject<'r>> = Vec::with_capacity(walkers.len());
        for spawned in walkers {
            let Ok(typed) = names.unit_type(spawned, rules) else {
                self.drop(spawned, SupplyDropCause::NoType);
                return Vec::new();
            };
            if typed.imported.life().is_none() {
                self.drop(spawned, SupplyDropCause::Unliving);
                return Vec::new();
            }
            units.push(NamedObject {
                name: typed.name.clone(),
                object: typed.object,
            });
        }
        let unit_types = units.iter().map(|unit| unit.name.declared().clone());
        let spawns = SpawnsData {
            unit_types: unit_types.collect(),
            count,
            replace_ms,
            led: spawning == Spawning::Led,
        };
        if spawning == Spawning::Led {
            self.drop(producer.object, SupplyDropCause::LedSpawns);
        }
        self.spawns.insert(producer.name.clone(), spawns);
        units
    }

    /// Drops each spawn of `object`, a building of no production, so of no exit to send its
    /// units by (09-zero-hour.md, A free unit needs an exit), its kept spawn but a leader's, which
    /// buds from its spawns (04-capabilities/production.md, A later spawn buds from the nearest
    /// spawn).
    pub(crate) fn strand(&mut self, rules: &ZeroHourRules, object: &IniObject, spawning: Spawning) {
        let kept = object
            .supply()
            .kept()
            .filter(|_| spawning == Spawning::ByExit)
            .map(|kept| kept.templates[0].as_str());
        for template in object.supply().one_shots().chain(kept) {
            if let Some(spawned) = self.walker(rules, template) {
                self.drop(spawned, SupplyDropCause::NoExit);
            }
        }
    }

    /// The object `template` of `rules`, when it gives a unit type that walks; else none, and
    /// its spawn dropped.
    fn walker<'r>(&mut self, rules: &'r ZeroHourRules, template: &str) -> Option<&'r IniObject> {
        let Some(spawned) = rules.objects.object(template.as_bytes()) else {
            self.drop_named(template, SupplyDropCause::NoObject);
            return None;
        };
        let cause = match ImportedType::of(spawned, rules) {
            Ok(imported) if imported.walks() => return Some(spawned),
            Ok(_) => SupplyDropCause::NoWalker,
            Err(_) => SupplyDropCause::NoType,
        };
        self.drop(spawned, cause);
        None
    }

    /// The node of `dock`, of money: its boxes' worth at the box value of `rules` each, which
    /// stays unless it goes as its last box goes; a cause for a dock of no box, or of more money
    /// than a node holds.
    fn node(dock: SupplyDock, rules: &ZeroHourRules) -> Result<NodeData, SupplyDropCause> {
        let boxes = u32::try_from(dock.boxes)
            .ok()
            .and_then(NonZeroU32::new)
            .ok_or(SupplyDropCause::NoBoxes)?;
        let amount = boxes
            .checked_mul(rules.game_data.box_value)
            .ok_or(SupplyDropCause::BadNumber)?;
        Ok(NodeData {
            resource: rules.names.money.clone(),
            amount,
            empty: if dock.delete_when_empty {
                EmptyNode::Goes
            } else {
                EmptyNode::Stays
            },
        })
    }

    /// The gather of `gatherer`: of money, a box of the box value of `rules` at each windup of
    /// its warehouse delay, up to its `boxes`, within the dock range of a dock, standing its
    /// center delay at a drop-off, and looking within its scan distance for another dock, aimed
    /// at every unit, as the docks stand on the neutral team and the node book picks the docks
    /// among them (09-zero-hour.md, A gatherer docks as a dozer builds); a cause for a number
    /// past what the engine holds.
    fn imported(
        gatherer: SupplyGatherer,
        boxes: NonZeroU32,
        rules: &ZeroHourRules,
    ) -> Result<ImportedGather, SupplyDropCause> {
        let take = boxes
            .checked_mul(rules.game_data.box_value)
            .ok_or(SupplyDropCause::BadNumber)?;
        let ms = |ms| game_time::of_ms(ms).map_err(|_| SupplyDropCause::BadNumber);
        let windup_ms = i64::try_from(ms(gatherer.warehouse_delay_ms)?)
            .map_err(|_| SupplyDropCause::BadNumber)?;
        let bounce = game_float::to_num(gatherer.scan_distance)
            .ok()
            .and_then(Distance::new)
            .ok_or(SupplyDropCause::BadNumber)?;
        let filter = rules.names.every.clone();
        let keys = ActionKeys {
            windup_ms: Some(Ranked::One(Number::from(windup_ms))),
            range: Some(Ranked::One(RangeField::Range(ActionRange::meters(
                DOCK_RANGE,
            )))),
            ..ActionKeys::new(Targeting::Unit(filter))
        };
        let gather = GatherData {
            resource: rules.names.money.clone(),
            take,
            unit: Some(rules.game_data.box_value),
            bounce: Some(bounce),
            deliver_ms: Some(ms(gatherer.center_delay_ms)?),
        };
        Ok(ImportedGather { keys, gather })
    }

    /// The keys of the hack of `hacker`, a cast of no target: its unpack, spread by its variation
    /// each way, then a channel of no end that pays its regular cash of money at each cash
    /// update, and an order's wait of its pack (`HackInternetState::update`); a cause for a time
    /// below 0 or past what the engine holds, a cash below 0, or a variation of 1 or more.
    fn hacking(hacker: InternetHack, rules: &ZeroHourRules) -> Result<ActionKeys, SupplyDropCause> {
        let ms = |ms| {
            game_time::of_ms(ms)
                .ok()
                .and_then(|ms| i64::try_from(ms).ok())
                .ok_or(SupplyDropCause::BadNumber)
        };
        let some_ms = |ms: i64| (ms > 0).then(|| Ranked::One(Number::from(ms)));
        if hacker.cash.iter().any(|&amount| amount < 0) {
            return Err(SupplyDropCause::BadNumber);
        }
        let spread = game_float::to_num(hacker.variation)
            .ok()
            .filter(|spread| (Num::ZERO..Num::ONE).contains(spread))
            .ok_or(SupplyDropCause::BadNumber)?;
        let tick_ms = some_ms(ms(hacker.cash_delay_ms)?).ok_or(SupplyDropCause::BadNumber)?;
        let channel = ChannelData {
            duration_ms: None,
            tick_ms,
            damage_interrupts: false,
            windup_spread: (spread > Num::ZERO)
                .then_some(Ranked::One(Number::Value(Scalar::Decimal(spread)))),
            wind_down_ms: some_ms(ms(hacker.pack_ms)?),
            pays: Some(ChannelPaysData {
                resource: rules.names.money.clone(),
                track: None,
                amount: vec![Self::cash(hacker.cash)[0]],
            }),
        };
        Ok(ActionKeys {
            windup_ms: some_ms(ms(hacker.unpack_ms)?),
            channel: Some(channel),
            ..ActionKeys::new(Targeting::None)
        })
    }

    /// The cash a hack pays at each veterancy from regular, none below 0: the cash of its level
    /// or, where that is 0, of the nearest level below it that is not, else 1, as
    /// `HackInternetState::update` falls through its levels.
    fn cash(cash: [i32; 4]) -> [u32; 4] {
        let mut paid = [1; 4];
        let mut below = 1;
        for (level, &amount) in cash.iter().enumerate() {
            if let Ok(amount) = u32::try_from(amount)
                && amount > 0
            {
                below = amount;
            }
            paid[level] = below;
        }
        paid
    }

    /// Drops the supply of `object` for `cause`, once for each cause.
    fn drop(&mut self, object: &IniObject, cause: SupplyDropCause) {
        self.drop_named(&object.name, cause);
    }

    /// Drops the supply of the object `name` for `cause`, once for each cause.
    fn drop_named(&mut self, name: &str, cause: SupplyDropCause) {
        self.dropped.drop(name, cause);
    }
}

impl ImportedGather {
    /// Its table under `[actions]`.
    pub(crate) fn table(&self) -> TomlTable {
        self.keys.table(&self.gather)
    }
}

/// A gather of one box of one money, a near dock and no delay, for another module's tests.
#[cfg(test)]
impl ImportedGather {
    pub(crate) fn of_box() -> Self {
        let gatherer = SupplyGatherer {
            max_boxes: 1,
            center_delay_ms: 0,
            warehouse_delay_ms: 0,
            scan_distance: 100.0,
        };
        let mut rules = ZeroHourRules::default();
        rules.game_data.box_value = NonZeroU32::MIN;
        SupplyImport::imported(gatherer, NonZeroU32::MIN, &rules)
            .expect("a box of one money imports")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dock_or_a_gatherer_past_what_a_node_or_a_load_holds_is_dropped() {
        // 2³² ÷ 75 boxes are past a node of `u32`; the most just within it holds.
        let mut rules = ZeroHourRules::default();
        rules.game_data.box_value = NonZeroU32::new(75).unwrap();
        let dock = |boxes| SupplyDock {
            boxes,
            delete_when_empty: false,
        };
        let most = i32::try_from(u32::MAX / 75).unwrap();
        let held = SupplyImport::node(dock(most), &rules).unwrap();
        assert_eq!(held.amount.get(), u32::MAX / 75 * 75);
        assert_eq!(
            SupplyImport::node(dock(most + 1), &rules),
            Err(SupplyDropCause::BadNumber)
        );
        assert_eq!(
            SupplyImport::node(dock(-1), &rules),
            Err(SupplyDropCause::NoBoxes)
        );
        // A load of as many boxes holds as well, and a delay below 0 is no time.
        let gatherer = |delay| SupplyGatherer {
            max_boxes: 1,
            center_delay_ms: delay,
            warehouse_delay_ms: 0,
            scan_distance: 100.0,
        };
        let boxes = |count: i32| NonZeroU32::new(count.unsigned_abs()).unwrap();
        let cause = |gatherer, count| SupplyImport::imported(gatherer, count, &rules).err();
        assert_eq!(cause(gatherer(0), boxes(most)), None);
        assert_eq!(
            cause(gatherer(0), boxes(most + 1)),
            Some(SupplyDropCause::BadNumber)
        );
        assert_eq!(
            cause(gatherer(-1), boxes(1)),
            Some(SupplyDropCause::BadNumber)
        );
    }
}
