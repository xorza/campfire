use std::collections::{BTreeMap, BTreeSet};

use campfire_common::DeclaredName;
use campfire_declarations::{
    BookCx, BookError, BookPlace, NamedScripts, NamedTexts, ScriptParams, Section, SourceKind,
    UnitTypeData,
};
use campfire_script::{ScriptRole, UnknownName};
use campfire_sim::Capability;
use campfire_units::PoolId;
use campfire_values::{
    ActionId, EngineTag, ModifierId, NameKind, Named, Names, PackageIndex, Param, ResourceId,
    UnitType,
};

use crate::Actions;
use crate::action_data::ActionData;
use crate::action_kind::ActionKind;
use crate::actions_data::ActionsData;
use crate::cost_target::CostTarget;
use crate::delivery_section::DeliverySection;
use crate::error::{ActionProblem, SlotError};
use crate::list_hook::ListHook;
use crate::listed_effect::effect_data::EffectData;
use crate::listed_effect::effect_data::LaunchLists;
use crate::listed_effect::error::EffectRule;
use crate::listed_effect::listed_spec::EffectTo;
use crate::listed_effect::{EffectRuleCx, RuleAt};
use crate::slot_kind::SlotKind;
use crate::slot_kinds::SlotKinds;
use crate::targeting::Targeting;
use crate::type_slots_data::TypeSlotsData;

/// What the rules of one action read: the match's build, the action's package and id, and that
/// package's unit types.
#[derive(Debug, Clone, Copy)]
struct ActionAt<'c, 'a> {
    cx: &'c BookCx<'a>,
    package: PackageIndex,
    id: &'c DeclaredName,
    units: &'c BTreeMap<DeclaredName, UnitTypeData>,
}

impl Actions {
    /// Adds the script of each package's action, which serves the action and reads its params.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn named_scripts(
        cx: &BookCx<'_>,
        scripts: &mut NamedScripts,
    ) -> Result<(), BookError> {
        for (package, source) in cx.packages() {
            let actions = source.content.get::<ActionsData>();
            for action in actions.into_iter().flat_map(|actions| actions.0.values()) {
                if let Some(script) = &action.keys.script {
                    let params = ScriptParams::Named(action.keys.params.keys().cloned().collect());
                    scripts.name(package, script, ScriptRole::Action, params);
                }
            }
        }
        Ok(())
    }

    /// Adds the name of each package's action that gives one, a message of its package.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn named_texts(cx: &BookCx<'_>, messages: &mut NamedTexts) -> Result<(), BookError> {
        for (package, source) in cx.packages() {
            let actions = source.content.get::<ActionsData>();
            for (id, action) in actions.into_iter().flat_map(|actions| &actions.0) {
                if let Some(name) = &action.keys.name {
                    let at = BookPlace::Entry {
                        table: ActionsData::KEY,
                        name: id.clone(),
                    };
                    messages.name(package, at, name);
                }
            }
        }
        Ok(())
    }

    /// The rules of every package's actions and of its unit types' slots: each action's pipeline
    /// fields its kind takes, the capabilities its kind, its delivery and its effects use, its
    /// effects' units, the names of its costs, its modifiers and its params; each slot of a unit
    /// type holds an action once; and each action of an avatar package sits in a slot of the
    /// package's types.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let no_pools = Names::default();
        let pools = cx.names_if::<PoolId>()?;
        let pools = pools.map_or(&no_pools, |pools| &**pools);
        let no_resources = Names::default();
        let resources = cx.names_if::<ResourceId>()?;
        let resources = resources.map_or(&no_resources, |resources| &**resources);
        let modifiers = cx.names_if::<ModifierId>()?;
        let modifiers = modifiers.map(|modifiers| &**modifiers);
        for (package, source) in cx.packages() {
            let Some(actions) = source.content.get::<ActionsData>() else {
                continue;
            };
            for (id, action) in &actions.0 {
                let at = ActionAt {
                    cx,
                    package,
                    id,
                    units: source.units,
                };
                at.kind(action)?;
                at.effects(action)?;
                if let Some(delivery) = &action.delivery {
                    at.delivery(&**delivery)?;
                }
                let costs = action.cost_names();
                let mut costs = costs
                    .filter(|name| CostTarget::named(pools, resources, name.as_str()).is_none());
                if let Some(name) = costs.next() {
                    return Err(at.unknown(CostTarget::NAMES, name));
                }
                let held = |name: &&DeclaredName| {
                    modifiers.is_some_and(|modifiers| {
                        modifiers.named((package, name.as_str())).is_some()
                    })
                };
                if let Some(name) = action.modifiers().find(|name| !held(name)) {
                    return Err(at.unknown(ModifierId::NAMES, name));
                }
                if let Some(name) = action
                    .param_refs()
                    .find(|name| !action.keys.params.contains_key(*name))
                {
                    return Err(at.unknown(Param::NAMES, name));
                }
            }
        }
        Self::slots_rule(cx)
    }

    /// The rule of the unit types' slots: each of a kind the mode declares, holding actions of
    /// its type's package, each once; and each action of an avatar package sits in a slot of the
    /// avatar or of the package's types that stand.
    fn slots_rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let kinds = cx.mode_section::<SlotKinds>();
        for unit in cx.types() {
            let Some(slots) = unit.data.section::<TypeSlotsData>() else {
                continue;
            };
            let at = || BookPlace::UnitType(unit.place());
            let unknown =
                |of, name| BookError::unknown(unit.package, at(), UnknownName::new(of, name));
            let actions = cx.package(unit.package).content.get::<ActionsData>();
            let mut slotted = BTreeSet::new();
            for (kind, ids) in &slots.0 {
                if kinds.is_none_or(|kinds| kinds.names().named(kind.as_str()).is_none()) {
                    return Err(unknown(SlotKind::NAMES, kind));
                }
                for id in ids {
                    if actions.is_none_or(|actions| !actions.0.contains_key(id)) {
                        return Err(unknown(ActionId::NAMES, id));
                    }
                    if !slotted.insert(id) {
                        let problem = SlotError::Repeated(id.clone());
                        return Err(BookError::rule(unit.package, at(), problem));
                    }
                }
            }
        }
        for (package, source) in cx.packages() {
            let SourceKind::Avatar(avatar) = source.kind else {
                continue;
            };
            let standing = source.units.values().filter(|unit| !unit.delivers());
            let slots = standing
                .chain([avatar])
                .filter_map(|unit| unit.section::<TypeSlotsData>());
            let slotted: BTreeSet<&DeclaredName> =
                slots.flat_map(|slots| slots.0.values()).flatten().collect();
            let actions = source.content.get::<ActionsData>().into_iter();
            let mut actions = actions.flat_map(|actions| actions.0.keys());
            if let Some(id) = actions.find(|id| !slotted.contains(id)) {
                let at = BookPlace::Entry {
                    table: ActionsData::KEY,
                    name: id.clone(),
                };
                return Err(BookError::rule(package, at, ActionProblem::Unslotted));
            }
        }
        Ok(())
    }
}

impl ActionAt<'_, '_> {
    fn place(self) -> BookPlace {
        BookPlace::Entry {
            table: ActionsData::KEY,
            name: self.id.clone(),
        }
    }

    fn problem(self, problem: ActionProblem) -> BookError {
        BookError::rule(self.package, self.place(), problem)
    }

    fn unknown(self, of: NameKind, name: &DeclaredName) -> BookError {
        BookError::unknown(self.package, self.place(), UnknownName::new(of, name))
    }

    fn require(self, capability: Capability) -> Result<(), BookError> {
        self.cx.require(capability, self.package, || self.place())
    }

    /// Its kind: the pipeline's fields its section takes, and the capabilities its section
    /// needs. A clamp only of an aim at a point, and a hold only of a toggle or a channel.
    fn kind(self, action: &ActionData) -> Result<(), BookError> {
        if let Some(field) = action.misused() {
            return Err(self.problem(ActionProblem::KindField(field)));
        }
        if action.keys.clamp_to_range && action.keys.targeting != Targeting::Point {
            return Err(self.problem(ActionProblem::ClampAims));
        }
        if action.keys.hold.is_some()
            && action.keys.toggle.is_none()
            && action.keys.channel.is_none()
        {
            return Err(self.problem(ActionProblem::HoldAlone));
        }
        action
            .section
            .needs()
            .iter()
            .try_for_each(|&capability| self.require(capability))
    }

    /// Its delivery: of the capability of its form, and a unit type of its package.
    fn delivery(self, delivery: &dyn DeliverySection) -> Result<(), BookError> {
        self.require(delivery.capability())?;
        let name = delivery.unit_type();
        if !self.units.contains_key(name) {
            return Err(self.unknown(UnitType::NAMES, name));
        }
        Ok(())
    }

    /// Its effect lists: a hit's or an end's list only of an action that delivers, or of an
    /// attack, which reaches its target at once; and each list as `effect_list` checks it.
    fn effects(self, action: &ActionData) -> Result<(), BookError> {
        for (list, effects) in action.effect_lists() {
            let delivers = action.delivery.is_some() || action.kind() == ActionKind::Attack;
            if !effects.is_empty() && list != ListHook::OnResolve && !delivers {
                let rule = EffectRule::NoDelivery;
                return Err(self.problem(ActionProblem::Effect { list, rule }));
            }
            let reaches = match list {
                ListHook::OnResolve => matches!(action.keys.targeting, Targeting::Unit(_)),
                ListHook::OnHit => true,
                ListHook::OnEnd => false,
            };
            self.effect_list(action, list, effects, reaches)?;
        }
        Ok(())
    }

    /// The list `effects`, which runs before `list` and reaches a unit when `reaches`: each effect
    /// of a capability the mode declares, to a unit the list reaches, unless it takes the point an
    /// `on_resolve` aimed at a point reached, and each effect's own rules.
    fn effect_list(
        self,
        action: &ActionData,
        list: ListHook,
        effects: &[EffectData],
        reaches: bool,
    ) -> Result<(), BookError> {
        // An `on_resolve` aimed at a point reaches no unit, and a spawn there takes the point.
        let aims_point = list == ListHook::OnResolve
            && matches!(
                action.keys.targeting,
                Targeting::Point | Targeting::Direction
            );
        let cx = ListRules {
            at: self,
            action,
            list,
            reaches,
        };
        for effect in effects {
            let placed = aims_point && effect.does.takes_point();
            if effect.to == EffectTo::Reached && !reaches && !placed {
                return Err(cx.fail(EffectRule::NoUnit));
            }
            if let Some(capability) = effect.does.needs() {
                self.require(capability)?;
            }
            effect.does.rule(&cx)?;
        }
        Ok(())
    }
}

/// What the rules of a listed effect read: its action, in its package, and its list, `list`,
/// which reaches a unit when `reaches`.
#[derive(Debug, Clone, Copy)]
struct ListRules<'c, 'a> {
    at: ActionAt<'c, 'a>,
    action: &'c ActionData,
    list: ListHook,
    reaches: bool,
}

impl EffectRuleCx for ListRules<'_, '_> {
    fn reaches(&self) -> bool {
        self.reaches
    }

    fn package_type(&self, name: &DeclaredName) -> Option<&UnitTypeData> {
        self.at.units.get(name)
    }

    fn engine_tag(&self, tag: &DeclaredName) -> bool {
        EngineTag::named(self.at.cx.source().engine_tags, tag.as_str()).is_some()
    }

    fn fail(&self, rule: EffectRule) -> BookError {
        self.at.problem(ActionProblem::Effect {
            list: self.list,
            rule,
        })
    }

    fn unknown(&self, of: NameKind, name: &DeclaredName) -> BookError {
        self.at.unknown(of, name)
    }

    fn at(&self) -> RuleAt {
        RuleAt {
            package: self.at.package,
            place: self.at.place(),
        }
    }

    fn lists(&self, lists: &LaunchLists) -> Result<(), BookError> {
        let (at, action) = (self.at, self.action);
        at.effect_list(action, ListHook::OnHit, &lists.on_hit, true)?;
        at.effect_list(action, ListHook::OnEnd, &lists.on_end, false)
    }
}
