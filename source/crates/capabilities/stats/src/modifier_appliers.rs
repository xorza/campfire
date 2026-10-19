use campfire_actions::ActionsData;
use campfire_common::DeclaredName;
use campfire_declare::{BookCx, BookError, Fact};
use campfire_script::{AppliedModifier, Applies, ScriptUses};
use campfire_values::{PackageIndex, PackagePath};

use crate::error::Way;
use crate::modifiers_data::ModifiersData;
use crate::passive_data::PassiveData;

/// What applies each package's modifiers, as data and scripts name them, before the ways pass on:
/// a fact the modules whose data applies a modifier extend. A script applies the modifiers of
/// the literal names it gives the calls the registry marks as applying; `has_modifier` only names
/// a modifier.
#[derive(Debug, Default)]
pub struct ModifierAppliers {
    pub(crate) applied: Vec<Applied>,
    /// The modifiers that pass each of their ways on to another of their package's: an aura's,
    /// and those a modifier's script applies with its action.
    pub(crate) passes: Vec<Pass>,
}

/// A way that applies a modifier of a package.
#[derive(Debug)]
pub(crate) struct Applied {
    pub(crate) package: PackageIndex,
    pub(crate) modifier: String,
    pub(crate) way: Way,
}

/// That modifier `to` of a package is applied in each way `from` is.
#[derive(Debug)]
pub(crate) struct Pass {
    pub(crate) package: PackageIndex,
    pub(crate) from: DeclaredName,
    pub(crate) to: String,
}

impl Fact for ModifierAppliers {}

impl ModifierAppliers {
    /// What the actions and the modifiers of every package apply: an action's `hold`,
    /// `passive_modifier` and effect lists, and its script, with that action; a type that stands
    /// its `passive`, and the mode's script, with none; an aura's modifier, and a modifier's
    /// script, each way of that modifier.
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let mut appliers = Self::default();
        for (package, source) in cx.packages() {
            let actions = source.content.get::<ActionsData>();
            for (id, action) in actions.into_iter().flat_map(|actions| &actions.0) {
                let way = Way::Action(id.clone());
                for modifier in action.modifiers() {
                    appliers.apply(package, modifier.as_str(), way.clone());
                }
                if let Some(script) = &action.script {
                    appliers.script(cx, package, script, &way)?;
                }
            }
            let modifiers = source.content.get::<ModifiersData>();
            for (id, modifier) in modifiers.into_iter().flat_map(|modifiers| &modifiers.0) {
                if let Some(aura) = &modifier.aura {
                    appliers.pass(package, id, aura.modifier.as_str());
                }
                let Some(script) = &modifier.script else {
                    continue;
                };
                for applied in Self::applied(cx, package, script)? {
                    match applied.applies {
                        Applies::WithAction => appliers.pass(package, id, &applied.name),
                        Applies::WithoutAction => {
                            appliers.apply(package, &applied.name, Way::NoAction);
                        }
                    }
                }
            }
        }
        for unit in cx.standing() {
            if let Some(passive) = unit.data.section::<PassiveData>() {
                appliers.apply(unit.package, passive.0.as_str(), Way::NoAction);
            }
        }
        for mode in &cx.source().mode_scripts {
            appliers.script(cx, mode.package, mode.path, &Way::NoAction)?;
        }
        Ok(appliers)
    }

    /// Adds that `way` applies `modifier`, of the package at `package`.
    pub fn apply(&mut self, package: PackageIndex, modifier: &str, way: Way) {
        self.applied.push(Applied {
            package,
            modifier: modifier.to_owned(),
            way,
        });
    }

    /// Adds what the script at `path` of the package at `package` applies: with its action as
    /// `way` applies it, and with none as no action does; an error for a script the package
    /// does not hold.
    pub fn script(
        &mut self,
        cx: &BookCx<'_>,
        package: PackageIndex,
        path: &PackagePath,
        way: &Way,
    ) -> Result<(), BookError> {
        for applied in Self::applied(cx, package, path)? {
            let way = match applied.applies {
                Applies::WithAction => way.clone(),
                Applies::WithoutAction => Way::NoAction,
            };
            self.apply(package, &applied.name, way);
        }
        Ok(())
    }

    fn pass(&mut self, package: PackageIndex, from: &DeclaredName, to: &str) {
        self.passes.push(Pass {
            package,
            from: from.clone(),
            to: to.to_owned(),
        });
    }

    /// What the script at `path` of the package at `package` applies, as the load parsed it.
    fn applied<'c>(
        cx: &BookCx<'c>,
        package: PackageIndex,
        path: &PackagePath,
    ) -> Result<&'c [AppliedModifier], BookError> {
        let script = cx.script(package, path)?;
        let uses = cx.given::<ScriptUses>();
        let uses = uses.expect("the load gives the scripts' uses");
        Ok(&uses.of(script).applied)
    }
}
