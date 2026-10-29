use campfire_actions::ActionKind;
use campfire_common::DeclaredName;
use campfire_production::RequiresData;

use crate::zero_hour::additions::Additions;
use crate::zero_hour::combat_import::CombatImport;
use crate::zero_hour::production_import::{MadeSection, ProductionImport};
use crate::zero_hour::skirmish_start::FactionStart;

/// A faction's base, which a player slot that starts with one gets beside its start
/// (09-zero-hour.md, A base start): each type its start builders build but its faction building,
/// once each, in their command sets' order, and each type those buildings and its faction building
/// train, once each, in that order; its army, those units that neither build nor gather and that a
/// weapon arms or that assault with their riders (04-capabilities/interaction.md, A transport may
/// assault), which a bot trains and sends
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

/// What a faction's base is made of: the actions production converted, the weapons combat
/// converted, and what the stages gave each type, its command set's actions, its spawns, its
/// entry and its drop-off.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BaseSources<'a> {
    pub(crate) production: &'a ProductionImport,
    pub(crate) combat: &'a CombatImport,
    pub(crate) additions: &'a Additions,
}

impl FactionBase {
    /// The base of `faction`, from the command sets `sources` converted, but a train or a build
    /// that needs a modifier other than those of `held`, the perks it starts with, its army armed
    /// by a weapon combat converted or an entry of an assault.
    pub(crate) fn of(
        sources: BaseSources<'_>,
        faction: &FactionStart,
        held: &[DeclaredName],
    ) -> Self {
        let assaults = |unit: &DeclaredName| {
            let enter = sources
                .additions
                .get(unit)
                .and_then(|added| added.enter.as_ref());
            enter.is_some_and(|enter| enter.assault.is_some())
        };
        let building = &faction.building;
        let mut buildings: Vec<DeclaredName> = Vec::new();
        for builder in &faction.units {
            for made in sources.made(builder, ActionKind::Build, held) {
                if made != building && !buildings.contains(made) {
                    buildings.push(made.clone());
                }
            }
        }
        let mut units: Vec<DeclaredName> = Vec::new();
        for producer in [building].into_iter().chain(&buildings) {
            for made in sources.made(producer, ActionKind::Train, held) {
                if !units.contains(made) {
                    units.push(made.clone());
                }
            }
        }
        let army = units
            .iter()
            .filter(|unit| sources.fights(unit) && (sources.armed(unit) || assaults(unit)))
            .cloned()
            .collect();
        let gatherers = units
            .iter()
            .filter(|unit| sources.gathers(unit))
            .cloned()
            .collect();
        let drop_offs = [building]
            .into_iter()
            .chain(&buildings)
            .filter(|building| {
                let added = sources.additions.get(*building);
                added.is_some_and(|added| added.drop_off.is_some())
            })
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

impl BaseSources<'_> {
    /// Whether `unit`'s command set holds a gather.
    fn gathers(&self, unit: &DeclaredName) -> bool {
        let gathers = &self.production.supply.gathers;
        self.additions
            .commands(unit)
            .iter()
            .any(|id| gathers.contains_key(id))
    }

    /// Whether a weapon of combat arms `unit`, or, for a leader, one of its spawns, which fight
    /// in its place (09-zero-hour.md, A leader of armed spawns is armed).
    fn armed(&self, unit: &DeclaredName) -> bool {
        let spawns = self
            .additions
            .get(unit)
            .and_then(|added| added.spawns.as_ref());
        let led = spawns.filter(|spawns| spawns.led);
        self.combat.arms(unit)
            || led.is_some_and(|spawns| {
                spawns
                    .unit_types
                    .iter()
                    .any(|spawn| self.combat.arms(spawn))
            })
    }

    /// Whether `unit`'s command set holds trains and researches alone: none of its actions
    /// builds or gathers.
    fn fights(&self, unit: &DeclaredName) -> bool {
        self.additions.commands(unit).iter().all(|id| {
            let action = self.production.actions.get(id);
            action.is_some_and(|action| !matches!(action.made, MadeSection::Build(_)))
        })
    }

    /// The unit types the actions of `kind` of `producer`'s command set make, in its order, but a
    /// jet, made at a place of its airfield, which a base start does not hold.
    fn made<'a>(
        &'a self,
        producer: &DeclaredName,
        kind: ActionKind,
        held: &'a [DeclaredName],
    ) -> impl Iterator<Item = &'a DeclaredName> {
        let holds = move |requires: &Option<RequiresData>| {
            requires.iter().all(|requires| {
                requires
                    .modifiers
                    .iter()
                    .all(|modifier| held.contains(modifier))
            })
        };
        self.additions
            .commands(producer)
            .iter()
            .filter_map(|id| self.production.actions.get(id))
            .filter_map(move |action| match (&action.made, kind) {
                (MadeSection::Train(train), ActionKind::Train)
                    if !train.at_place && holds(&train.requires) =>
                {
                    Some(&train.unit_type)
                }
                (MadeSection::Build(build), ActionKind::Build) if holds(&build.requires) => {
                    Some(&build.unit_type)
                }
                _ => None,
            })
    }
}
