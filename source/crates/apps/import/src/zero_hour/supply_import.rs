use std::collections::BTreeMap;
use std::num::NonZeroU32;

use campfire_actions::{ActionKeys, ActionKind, ActionRange, RangeField, Targeting};
use campfire_codec::TomlTable;
use campfire_common::{DeclaredName, EnumAll};
use campfire_math::Num;
use campfire_production::{DropOffData, EmptyNode, GatherData, NodeData};
use campfire_values::{Distance, FilterData, Number, Ranked};

use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::object_ini::IniObject;
use crate::zero_hour::format::ini::object_ini::ini_supply::{SupplyDock, SupplyGatherer};
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::ProductionNames;
use crate::zero_hour::imported_type::{ImportedType, MONEY};
use crate::zero_hour::weapon_drop;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// Zero Hour's supply, of the unit types production visits (09-zero-hour.md, A box is
/// money): each dock a node of money, its boxes' worth; each supply center a drop-off of money;
/// each gatherer that walks a gather of its own numbers; the free units of each producer, its
/// one-shot spawns that walk; and what it drops, each once for each cause, in the order the
/// types are visited.
#[derive(Debug, Default)]
pub(crate) struct SupplyImport {
    pub(crate) gathers: BTreeMap<DeclaredName, ImportedGather>,
    pub(crate) nodes: BTreeMap<ImportName, NodeData>,
    pub(crate) drop_offs: BTreeMap<ImportName, DropOffData>,
    pub(crate) dropped: Vec<DroppedSupply>,
}

/// A gather as the mode's data writes it, `[actions.<id>]`: the pipeline's keys it gives, and its
/// kind's section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedGather {
    keys: ActionKeys,
    gather: GatherData,
}

/// A unit a building frees: its unit type's name, and its object's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FreedUnit {
    pub(crate) unit_type: DeclaredName,
    pub(crate) object: String,
}

/// A supply module the import does not convert, by its object's name, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedSupply {
    pub object: String,
    pub cause: SupplyDropCause,
}

/// Why the import drops a supply module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll)]
pub enum SupplyDropCause {
    /// A dock that holds no box.
    NoBoxes,
    /// A number past what the engine holds.
    BadNumber,
    /// A gatherer, or a unit a building spawns once, that does not walk, as the USA's Chinook
    /// flies (09-zero-hour.md, The USA's income waits for aircraft).
    NoWalker,
    /// A unit a building that trains nothing spawns once, which has no exit to leave.
    NoExit,
    /// A unit spawned once that no file defines, or that gives no unit type.
    NoObject,
}

/// How near a builder comes to its building's box, and a gatherer to its dock's, 10 m, the
/// pathfinder's cell: the dozer docks in the free cell beside the building nearest it
/// (09-zero-hour.md, Every command set's trains and builds are imported; 09-zero-hour.md, A
/// gatherer docks as a dozer builds).
pub(crate) const DOCK_RANGE: Num = Num::int(10);

/// What a gather aims at: every unit, as the docks stand on the neutral team, which the mode's
/// players regard as hostile; the node book picks the docks among them (09-zero-hour.md, A
/// gatherer docks as a dozer builds).
const DOCKS: &str = "all";

impl SupplyImport {
    /// Adds the node of `object` when it is a dock, and its drop-off when it is a supply center,
    /// by the box value of `rules`.
    pub(crate) fn dock(&mut self, rules: &ZeroHourRules, object: &IniObject) {
        let supply = object.supply();
        if supply.center {
            let money = Self::money();
            let drop_off = DropOffData {
                resources: vec![money],
            };
            self.drop_offs.insert(ImportName::object(object), drop_off);
        }
        let Some(dock) = supply.dock else {
            return;
        };
        match Self::node(dock, rules.game_data.box_value) {
            Ok(node) => {
                self.nodes.insert(ImportName::object(object), node);
            }
            Err(cause) => self.drop(object, cause),
        }
    }

    /// The gather of `object`, a unit type of `names`, when it is a gatherer that walks, by the
    /// box value of `rules`: its action's name, which goes in its command slots. An AI module of
    /// no box, as a Helix's `ChinookAIUpdate`, gathers nothing in the original either, so it is no
    /// gatherer.
    pub(crate) fn gather(
        &mut self,
        rules: &ZeroHourRules,
        object: &IniObject,
        names: &mut ProductionNames,
    ) -> Option<DeclaredName> {
        let gatherer = object.supply().gatherer?;
        let boxes = u32::try_from(gatherer.max_boxes)
            .ok()
            .and_then(NonZeroU32::new)?;
        let typed = names.unit_type(object, &rules.locomotors).ok()?;
        if !typed.imported.walks() {
            self.drop(object, SupplyDropCause::NoWalker);
            return None;
        }
        let imported = match Self::imported(gatherer, boxes, rules.game_data.box_value) {
            Ok(imported) => imported,
            Err(cause) => {
                self.drop(object, cause);
                return None;
            }
        };
        let id = format!("{}_{}", ActionKind::Gather.name(), typed.name.as_str());
        let id = DeclaredName::new(&id).expect("a kind and a type's name make a declared name");
        self.gathers.entry(id.clone()).or_insert(imported);
        Some(id)
    }

    /// The units `object`, a building that trains, frees: its one-shot spawns that walk, each
    /// typed in `names`, which production visits in turn.
    pub(crate) fn free(
        &mut self,
        rules: &ZeroHourRules,
        object: &IniObject,
        names: &mut ProductionNames,
    ) -> Vec<FreedUnit> {
        let mut freed = Vec::new();
        for template in object.supply().one_shots() {
            let Some(spawned) = self.walker(rules, template) else {
                continue;
            };
            match names.unit_type(spawned, &rules.locomotors) {
                Ok(typed) => freed.push(FreedUnit {
                    unit_type: typed.name,
                    object: spawned.name.clone(),
                }),
                Err(_) => self.drop(spawned, SupplyDropCause::NoObject),
            }
        }
        freed
    }

    /// Drops each one-shot spawn of `object`, a building that trains nothing, so has no exit to
    /// free its units by (09-zero-hour.md, A free unit needs an exit).
    pub(crate) fn strand(&mut self, rules: &ZeroHourRules, object: &IniObject) {
        for template in object.supply().one_shots() {
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
        let cause = match ImportedType::of(spawned, &rules.locomotors) {
            Ok(imported) if imported.walks() => return Some(spawned),
            Ok(_) => SupplyDropCause::NoWalker,
            Err(_) => SupplyDropCause::NoObject,
        };
        self.drop(spawned, cause);
        None
    }

    /// The node of `dock`, of money: its boxes' worth at `box_value` each, which stays unless it
    /// goes as its last box goes; a cause for a dock of no box, or of more money than a node
    /// holds.
    fn node(dock: SupplyDock, box_value: NonZeroU32) -> Result<NodeData, SupplyDropCause> {
        let boxes = u32::try_from(dock.boxes)
            .ok()
            .and_then(NonZeroU32::new)
            .ok_or(SupplyDropCause::NoBoxes)?;
        let amount = boxes
            .checked_mul(box_value)
            .ok_or(SupplyDropCause::BadNumber)?;
        Ok(NodeData {
            resource: Self::money(),
            amount,
            empty: if dock.delete_when_empty {
                EmptyNode::Goes
            } else {
                EmptyNode::Stays
            },
        })
    }

    /// The gather of `gatherer`: of money, a box of `box_value` at each windup of its warehouse
    /// delay, up to its `boxes`, within the dock range of a dock, standing its center delay at a
    /// drop-off, and looking within its scan distance for another dock; a cause for a number past
    /// what the engine holds.
    fn imported(
        gatherer: SupplyGatherer,
        boxes: NonZeroU32,
        box_value: NonZeroU32,
    ) -> Result<ImportedGather, SupplyDropCause> {
        let take = boxes
            .checked_mul(box_value)
            .ok_or(SupplyDropCause::BadNumber)?;
        let ms = |ms| weapon_drop::game_ms(ms).map_err(|_| SupplyDropCause::BadNumber);
        let windup_ms = i64::try_from(ms(gatherer.warehouse_delay_ms)?)
            .map_err(|_| SupplyDropCause::BadNumber)?;
        let bounce = game_float::to_num(gatherer.scan_distance)
            .ok()
            .and_then(Distance::new)
            .ok_or(SupplyDropCause::BadNumber)?;
        let filter = FilterData::parse(DOCKS).expect("the import's filters parse");
        let keys = ActionKeys {
            windup_ms: Some(Ranked::One(Number::from(windup_ms))),
            range: Some(Ranked::One(RangeField::Range(ActionRange::meters(
                DOCK_RANGE,
            )))),
            ..ActionKeys::new(ActionKind::Gather, Targeting::Unit(filter))
        };
        let gather = GatherData {
            resource: Self::money(),
            take,
            unit: Some(box_value),
            bounce: Some(bounce),
            deliver_ms: Some(ms(gatherer.center_delay_ms)?),
        };
        Ok(ImportedGather { keys, gather })
    }

    /// The player resource of every node, drop-off and gather.
    fn money() -> DeclaredName {
        DeclaredName::new(MONEY).expect("a resource is a declared name")
    }

    /// Drops the supply of `object` for `cause`, once for each cause.
    fn drop(&mut self, object: &IniObject, cause: SupplyDropCause) {
        self.drop_named(&object.name, cause);
    }

    /// Drops the supply of the object `name` for `cause`, once for each cause.
    fn drop_named(&mut self, name: &str, cause: SupplyDropCause) {
        let dropped = DroppedSupply {
            object: name.to_owned(),
            cause,
        };
        if !self.dropped.contains(&dropped) {
            self.dropped.push(dropped);
        }
    }
}

impl ImportedGather {
    /// Its table under `[actions]`.
    pub(crate) fn table(&self) -> TomlTable {
        self.keys.table(&self.gather)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dock_or_a_gatherer_past_what_a_node_or_a_load_holds_is_dropped() {
        // 2³² ÷ 75 boxes are past a node of `u32`; the most just within it holds.
        let box_value = NonZeroU32::new(75).unwrap();
        let dock = |boxes| SupplyDock {
            boxes,
            delete_when_empty: false,
        };
        let most = i32::try_from(u32::MAX / 75).unwrap();
        let held = SupplyImport::node(dock(most), box_value).unwrap();
        assert_eq!(held.amount.get(), u32::MAX / 75 * 75);
        assert_eq!(
            SupplyImport::node(dock(most + 1), box_value),
            Err(SupplyDropCause::BadNumber)
        );
        assert_eq!(
            SupplyImport::node(dock(-1), box_value),
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
        let cause = |gatherer, count| SupplyImport::imported(gatherer, count, box_value).err();
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
