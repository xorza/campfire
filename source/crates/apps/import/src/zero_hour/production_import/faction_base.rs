use campfire_actions::ActionKind;
use campfire_common::DeclaredName;

use crate::zero_hour::combat_import::CombatImport;
use crate::zero_hour::contain_import::ContainImport;
use crate::zero_hour::production_import::ProductionImport;
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

/// What a faction's base is made of: the command sets production converted, the weapons combat
/// converted, and the containers' entries.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BaseSources<'a> {
    pub(crate) production: &'a ProductionImport,
    pub(crate) combat: &'a CombatImport,
    pub(crate) contains: &'a ContainImport,
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
        let BaseSources {
            production,
            combat,
            contains,
        } = sources;
        let assaults = |unit: &DeclaredName| {
            (contains.enters.iter())
                .any(|(name, enter)| name.declared() == unit && enter.assault.is_some())
        };
        let building = &faction.building;
        let mut buildings: Vec<DeclaredName> = Vec::new();
        for builder in &faction.units {
            for made in production.made(builder, ActionKind::Build, held) {
                if made != building && !buildings.contains(made) {
                    buildings.push(made.clone());
                }
            }
        }
        let mut units: Vec<DeclaredName> = Vec::new();
        for producer in [building].into_iter().chain(&buildings) {
            for made in production.made(producer, ActionKind::Train, held) {
                if !units.contains(made) {
                    units.push(made.clone());
                }
            }
        }
        let army = units
            .iter()
            .filter(|unit| {
                production.fights(unit) && (production.armed(unit, combat) || assaults(unit))
            })
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
