use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use campfire_capabilities::{
    ActionData, Applies, DeclaredName, DeliveryData, PackagePath, UnitTypeFile,
};

use crate::package_view::{PackageView, ViewKind};

/// A way a modifier is applied, by design 04's list: with an action of its package, at each of
/// the action's ranks, or with no action, at rank 1.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Way {
    Action(DeclaredName),
    NoAction,
}

impl fmt::Display for Way {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Way::Action(action) => write!(f, "action {action}"),
            Way::NoAction => f.write_str("no action"),
        }
    }
}

/// The ways each modifier of a package is applied: an action's `hold`, `passive_modifier` and
/// effect lists, the `inside` of the area it delivers, and its script's `ctx.add_modifier`, with
/// that action; a standing unit type's `passive`, `ctx.add_player_modifier`, and the
/// `ctx.add_modifier` of the mode's script or an AI's, with none; and the aura of a modifier, and
/// the `ctx.add_modifier` of a modifier's script, with each way of that modifier, passed on until
/// nothing changes. A script's ways are those of the literal names it gives the calls the
/// registry marks as applying; `has_modifier` only names a modifier.
#[derive(Debug, Default)]
pub(crate) struct ModifierWays<'a> {
    ways: BTreeMap<&'a str, BTreeSet<Way>>,
}

impl<'a> ModifierWays<'a> {
    /// The ways of the modifiers of `view`, whose mode's script is `mode_script` when it is the
    /// mode.
    pub(crate) fn of(view: PackageView<'a>, mode_script: Option<&'a PackagePath>) -> Self {
        let package = view.package;
        let content = view.content;
        let mut found = ModifierWays::default();
        let applied = |path: &PackagePath| {
            package
                .script(path)
                .into_iter()
                .flat_map(|script| script.facts.applied())
        };
        for (id, action) in &content.actions {
            let way = Way::Action(id.clone());
            let inside = match &action.delivery {
                Some(DeliveryData::Area { unit_type }) => content
                    .units
                    .get(unit_type)
                    .and_then(|unit_type| unit_type.area.as_ref()),
                _ => None,
            };
            let named = action
                .modifiers()
                .chain(inside.into_iter().flat_map(|area| area.inside.modifiers()));
            for modifier in named {
                found.add(modifier.as_str(), &way);
            }
            for (modifier, applies) in action.script.iter().flat_map(applied) {
                let way = match applies {
                    Applies::WithAction => &way,
                    Applies::WithoutAction => &Way::NoAction,
                };
                found.add(modifier, way);
            }
        }
        let standing: Vec<&UnitTypeFile> = match view.kind {
            ViewKind::Mode => {
                let units = content.units.values();
                units.filter(|unit| !unit.delivers()).collect()
            }
            ViewKind::Avatar(avatar) => vec![&avatar.unit],
            ViewKind::Loadout => Vec::new(),
        };
        let ai = standing
            .iter()
            .filter_map(|unit| unit.orders.as_ref().map(|orders| &orders.ai));
        for path in mode_script.into_iter().chain(ai) {
            for (modifier, _) in applied(path) {
                found.add(modifier, &Way::NoAction);
            }
        }
        for passive in standing.iter().filter_map(|unit| unit.passive.as_ref()) {
            found.add(passive.as_str(), &Way::NoAction);
        }
        let mut passes: Vec<(&str, &str)> = Vec::new();
        for (id, modifier) in &content.modifiers {
            if let Some(aura) = &modifier.aura {
                passes.push((id.as_str(), aura.modifier.as_str()));
            }
            for (to, applies) in modifier.script.iter().flat_map(applied) {
                match applies {
                    Applies::WithAction => passes.push((id.as_str(), to)),
                    Applies::WithoutAction => found.add(to, &Way::NoAction),
                }
            }
        }
        found.pass_on(&passes);
        found
    }

    /// The ways of modifier `id`; none for one nothing applies.
    pub(crate) fn of_modifier(&self, id: &str) -> impl Iterator<Item = &Way> {
        self.ways.get(id).into_iter().flatten()
    }

    /// The actions of `actions` that apply modifier `id`, one for each of its ways with an
    /// action.
    pub(crate) fn actions_of<'m>(
        &self,
        id: &str,
        actions: &'m BTreeMap<DeclaredName, ActionData>,
    ) -> impl Iterator<Item = &'m ActionData> {
        self.of_modifier(id).filter_map(|way| match way {
            Way::Action(action) => actions.get(action),
            Way::NoAction => None,
        })
    }

    fn add(&mut self, modifier: &'a str, way: &Way) {
        self.ways.entry(modifier).or_default().insert(way.clone());
    }

    /// Gives each `to` of `passes` every way of its `from`, until no pass adds one.
    fn pass_on(&mut self, passes: &[(&'a str, &'a str)]) {
        let mut given = Vec::new();
        loop {
            let mut added = false;
            for &(from, to) in passes {
                given.clear();
                given.extend(self.of_modifier(from).cloned());
                let ways = self.ways.entry(to).or_default();
                for way in given.drain(..) {
                    added |= ways.insert(way);
                }
            }
            if !added {
                return;
            }
        }
    }
}
