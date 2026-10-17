use std::collections::{BTreeMap, BTreeSet};

use campfire_common::DeclaredName;
use campfire_declare::{
    BookCx, BookError, BookPlace, NamedMessages, NamedScripts, ScriptParams, Section, SourceKind,
    UnitTypeData,
};
use campfire_players::ResourceBook;
use campfire_script::{NameKind, ScriptRole, UnknownName};
use campfire_sim::Capability;
use campfire_stats::{ModifiersData, PassiveData, PoolNames};
use campfire_units::CollisionData;
use campfire_values::{EngineTag, Names, PackageIndex};

use crate::Actions;
use crate::action_data::ActionData;
use crate::action_data::error::EffectRule;
use crate::action_kind::ActionKind;
use crate::actions_data::ActionsData;
use crate::cost_target::CostTarget;
use crate::delivery_data::DeliveryData;
use crate::effect_data::{
    DoesData, EffectData, EffectTo, LaunchFields, MoveData, PurgeFields, SpawnFields,
};
use crate::error::{ActionProblem, AppliedError, KindError, SlotError};
use crate::kind_data::KindData;
use crate::list_hook::ListHook;
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
                if let Some(script) = &action.script {
                    let params = ScriptParams::Named(action.params.keys().cloned().collect());
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
    pub(crate) fn named_messages(
        cx: &BookCx<'_>,
        messages: &mut NamedMessages,
    ) -> Result<(), BookError> {
        for (package, source) in cx.packages() {
            let actions = source.content.get::<ActionsData>();
            for (id, action) in actions.into_iter().flat_map(|actions| &actions.0) {
                if let Some(name) = &action.name {
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

    /// The rule of the passives: a modifier is the passive of one owner of its package at most,
    /// of its unit types by `passive` and its actions by `passive_modifier`, as two would share
    /// its hold.
    pub(crate) fn passives_rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        for (package, source) in cx.packages() {
            let types = cx.types().iter().filter(|unit| unit.package == package);
            let types = types.filter_map(|unit| {
                let passive = unit.data.section::<PassiveData>()?;
                Some((BookPlace::UnitType(unit.place()), &passive.0))
            });
            let actions = source.content.get::<ActionsData>();
            let actions = actions.into_iter().flat_map(|actions| &actions.0);
            let actions = actions.filter_map(|(id, action)| {
                let at = BookPlace::Entry {
                    table: ActionsData::KEY,
                    name: id.clone(),
                };
                Some((at, action.passive_modifier.as_ref()?))
            });
            let mut held: BTreeMap<&DeclaredName, BookPlace> = BTreeMap::new();
            for (owner, modifier) in types.chain(actions) {
                if let Some(first) = held.get(modifier) {
                    let problem = AppliedError::SharedPassive {
                        modifier: modifier.clone(),
                        first: first.clone(),
                    };
                    return Err(BookError::rule(package, owner, problem));
                }
                held.insert(modifier, owner);
            }
        }
        Ok(())
    }

    /// The rules of every package's actions and of its unit types' slots: each action's fields of
    /// its kind, the capabilities its kind, its delivery and its effects use, its effects' units,
    /// the names of its costs, its modifiers and its params; each slot of a unit type holds an
    /// action once; and each action of an avatar package sits in a slot of the package's types.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let no_pools = Names::default();
        let pools = cx.fact_if::<PoolNames>()?;
        let pools = pools.map_or(&no_pools, |pools| &*pools.0);
        let no_resources = Names::default();
        let resources = cx.book_if::<ResourceBook>()?;
        let resources = resources.map_or(&no_resources, |resources| &*resources.0);
        for (package, source) in cx.packages() {
            let Some(actions) = source.content.get::<ActionsData>() else {
                continue;
            };
            let modifiers = source.content.get::<ModifiersData>();
            for (id, action) in &actions.0 {
                let at = ActionAt {
                    cx,
                    package,
                    id,
                    units: source.units,
                };
                at.kind(action)?;
                at.weapon_fields(action)?;
                at.effects(action)?;
                if let Some(delivery) = &action.delivery {
                    at.delivery(action, delivery)?;
                }
                let costs = action.cost_names();
                let mut costs = costs
                    .filter(|name| CostTarget::named(pools, resources, name.as_str()).is_none());
                if let Some(name) = costs.next() {
                    return Err(at.unknown(NameKind::Cost, name));
                }
                let held = |name: &&DeclaredName| {
                    modifiers.is_some_and(|modifiers| modifiers.0.contains_key(*name))
                };
                if let Some(name) = action.modifiers().find(|name| !held(name)) {
                    return Err(at.unknown(NameKind::Modifier, name));
                }
                if let Some(name) = action
                    .param_refs()
                    .find(|name| !action.params.contains_key(*name))
                {
                    return Err(at.unknown(NameKind::Param, name));
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
                    return Err(unknown(NameKind::SlotKind, kind));
                }
                for id in ids {
                    if actions.is_none_or(|actions| !actions.0.contains_key(id)) {
                        return Err(unknown(NameKind::Ability, id));
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

    /// Its kind, one the release runs, with the fields the table of action fields lets its kind
    /// take and the capabilities of its kind: a cast's `abilities`; a use's `interaction` and
    /// `abilities`, whose casts run it; a train's `production`; a build's and a gather's
    /// `production` and `navigation`, which tests a box for room and walks a worker; an attack's
    /// `combat`. A clamp only of an aim at a point, and a hold only of a toggle or a channel.
    fn kind(self, action: &ActionData) -> Result<(), BookError> {
        let kind = match KindData::of(action) {
            Ok(kind) => Some(kind),
            Err(KindError::NotRun(_)) => None,
            Err(KindError::Field(field)) => {
                return Err(self.problem(ActionProblem::KindField(field)));
            }
        };
        if action.clamp_to_range && action.targeting != Targeting::Point {
            return Err(self.problem(ActionProblem::ClampAims));
        }
        if action.hold.is_some() && action.toggle.is_none() && action.channel.is_none() {
            return Err(self.problem(ActionProblem::HoldAlone));
        }
        let Some(kind) = kind else {
            return Err(self.problem(ActionProblem::NotRun(action.kind)));
        };
        let needs: &[Capability] = match kind {
            KindData::Cast => &[Capability::Abilities],
            KindData::Use => &[Capability::Interaction, Capability::Abilities],
            KindData::Train { .. } => &[Capability::Production],
            KindData::Build { .. } | KindData::Gather { .. } => {
                &[Capability::Production, Capability::Navigation]
            }
            KindData::Attack { .. } => &[Capability::Combat],
        };
        needs
            .iter()
            .try_for_each(|&capability| self.require(capability))
    }

    /// Its weapon's fields that its delivery holds: a splash goes off where a projectile of its
    /// delivery ends, a scatter aims a projectile of its delivery, and a speed delays the hit of a
    /// weapon of no delivery (04-capabilities/combat.md, An attack may splash;
    /// 04-capabilities/combat.md, An attack may scatter; 04-capabilities/combat.md, A slow weapon
    /// delays its damage).
    fn weapon_fields(self, action: &ActionData) -> Result<(), BookError> {
        let projectile = matches!(action.delivery, Some(DeliveryData::Projectile { .. }));
        if action.splash.is_some() && !projectile {
            return Err(self.problem(ActionProblem::Splash));
        }
        if action.scatter.is_some() && !projectile {
            return Err(self.problem(ActionProblem::Scatter));
        }
        if action.speed.is_some() && action.delivery.is_some() {
            return Err(self.problem(ActionProblem::Speed));
        }
        Ok(())
    }

    /// Its delivery: of the capability of its kind, a unit type of its package, and, for an
    /// attack, no area, as a weapon's is a homing projectile or a curve.
    fn delivery(self, action: &ActionData, delivery: &DeliveryData) -> Result<(), BookError> {
        let capability = match delivery {
            DeliveryData::Projectile { .. } => Capability::Projectiles,
            DeliveryData::Area { .. } => Capability::Areas,
        };
        self.require(capability)?;
        let name = delivery.unit_type();
        if !self.units.contains_key(name) {
            return Err(self.unknown(NameKind::UnitType, name));
        }
        if action.kind == ActionKind::Attack && matches!(delivery, DeliveryData::Area { .. }) {
            return Err(self.problem(ActionProblem::Weapon));
        }
        Ok(())
    }

    /// Its effect lists: a hit's or an end's list only of an action that delivers, or of an
    /// attack, which reaches its target at once; and each list as `effect_list` checks it.
    fn effects(self, action: &ActionData) -> Result<(), BookError> {
        for (list, effects) in action.effect_lists() {
            let delivers = action.delivery.is_some() || action.kind == ActionKind::Attack;
            if !effects.is_empty() && list != ListHook::OnResolve && !delivers {
                let rule = EffectRule::NoDelivery;
                return Err(self.problem(ActionProblem::Effect { list, rule }));
            }
            let reaches = match list {
                ListHook::OnResolve => matches!(action.targeting, Targeting::Unit(_)),
                ListHook::OnHit => true,
                ListHook::OnEnd => false,
            };
            self.effect_list(action, list, effects, reaches)?;
        }
        Ok(())
    }

    /// The list `effects`, which runs before `list` and reaches a unit when `reaches`: each effect
    /// of a capability the mode declares, to a unit the list reaches; a purge of no engine's tag;
    /// a launch of an area type of its package, whose `on_hit` reaches a unit and whose `on_end`
    /// does not; and a spawn of a type of its package that stands, of no box body.
    fn effect_list(
        self,
        action: &ActionData,
        list: ListHook,
        effects: &[EffectData],
        reaches: bool,
    ) -> Result<(), BookError> {
        let fail = |rule| Err(self.problem(ActionProblem::Effect { list, rule }));
        // An `on_resolve` aimed at a point reaches no unit, and a spawn there takes the point.
        let aims_point = list == ListHook::OnResolve
            && matches!(action.targeting, Targeting::Point | Targeting::Direction);
        for effect in effects {
            let placed = aims_point && matches!(effect.does, DoesData::Spawn(SpawnFields { .. }));
            if effect.to == EffectTo::Reached && !reaches && !placed {
                return fail(EffectRule::NoUnit);
            }
            match &effect.does {
                DoesData::Planned(_) => {}
                DoesData::Damage(_) | DoesData::Heal(_) | DoesData::Restore(_) => {
                    self.require(Capability::Combat)?;
                }
                DoesData::Modifier(_) => self.require(Capability::Stats)?,
                DoesData::Xp(_) => self.require(Capability::Progression)?,
                DoesData::Purge(PurgeFields { tag }) => {
                    self.require(Capability::Stats)?;
                    if EngineTag::named(self.cx.source().engine_tags, tag.as_str()).is_some() {
                        return fail(EffectRule::EngineTag(tag.clone()));
                    }
                }
                DoesData::Move(moves) => {
                    self.require(Capability::Navigation)?;
                    let other = match moves {
                        MoveData::Dash { to, .. } => to,
                        MoveData::KnockBack { from, .. } => from,
                    };
                    if *other == EffectTo::Reached && !reaches {
                        return fail(EffectRule::NoUnit);
                    }
                }
                DoesData::Launch(LaunchFields {
                    area,
                    on_hit,
                    on_end,
                }) => {
                    self.require(Capability::Areas)?;
                    if !self.units.contains_key(area) {
                        return Err(self.unknown(NameKind::UnitType, area));
                    }
                    self.effect_list(action, ListHook::OnHit, on_hit, true)?;
                    self.effect_list(action, ListHook::OnEnd, on_end, false)?;
                }
                DoesData::Spawn(SpawnFields { unit_type, .. }) => {
                    let spawned = self.units.get(unit_type).filter(|unit| !unit.delivers());
                    let Some(spawned) = spawned else {
                        return Err(self.unknown(NameKind::UnitType, unit_type));
                    };
                    let collision = spawned.section::<CollisionData>();
                    if collision.is_some_and(|collision| collision.form.is_box()) {
                        return fail(EffectRule::SpawnBox);
                    }
                }
            }
        }
        Ok(())
    }
}
