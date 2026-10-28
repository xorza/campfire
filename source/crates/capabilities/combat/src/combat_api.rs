use campfire_actions::{ActionData, ActionsColumn};
use campfire_codec::StructFields;
use campfire_math::Num;
use campfire_script::{
    ApiBuilder, ApiHolder, ApiVersion, CapabilityRefusal, Checked, Ctx, MemberDecl, RoleSet,
    ScriptTicks, ScriptValue, ScriptWindow, Status, Unit,
};
use campfire_values::{Block, TagProperty};
use campfire_visibility::DataKind;

use crate::attack_data::AttackData;
use crate::combat_column::CombatColumn;
use crate::combat_data::CombatData;
use crate::combat_effect::CombatEffect;
use crate::damage_handle::DamageHandle;
use crate::error::CombatRefusal;
use crate::heal_handle::HealHandle;
use crate::mode_combat_data::{LeechData, ModeCombatData};
use crate::turret::turret_data::TurretData;

/// The script API of `combat` beside its effects' calls: `ctx.attack_hit`, `ctx.kill`,
/// `ctx.respawn`, and the `Damage` and `Heal` handles.
#[derive(Debug)]
pub struct CombatApi;

impl CombatApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        DamageHandle::register(api);
        HealHandle::register(api);
        Self::register_unit(api);
        let call = |name, signature, description| MemberDecl::call(name, signature, description);
        let kill = call(
            "kill",
            &[&["unit"]],
            "takes `unit`'s life whole: it dies as the next tick's deaths find it, with no killer; \
             nothing for a unit that does not live",
        )
        .roles(RoleSet::MODE);
        let respawn = MemberDecl::mode_call(
            "respawn",
            &[&["unit", "ms"]],
            "brings back `unit`, dead and of a type that stays, `ms` from the call",
        );
        let attack_hit = call(
            "attack_hit",
            &[&["target"]],
            "an extra attack of the acting unit on `target`, with the weapon it would choose against \
             `target` at any distance, struck at once: no crit, no `on_attack`, and nothing when no \
             weapon selects it",
        )
        .roles(RoleSet::ACTING);
        api.call(attack_hit, |ctx: &mut Ctx, target: Unit| {
            Self::attack_hit(ctx, &target)
        })
        .call(kill, |ctx: &mut Ctx, unit: Unit| {
            ctx.queue(CombatEffect::Kill { unit: unit.id })
        })
        .call(respawn, |ctx: &mut Ctx, unit: Unit, ms: ScriptTicks| {
            Self::respawn(ctx, &unit, ms)
        })
        .tag_property(
            TagProperty::Blocks(Block::Attack),
            Status::Runs(ApiVersion::FIRST),
        )
        .tag_property(
            TagProperty::Blocks(Block::Target),
            Status::Runs(ApiVersion::FIRST),
        )
        .tag_property(
            TagProperty::Blocks(Block::Damage),
            Status::Runs(ApiVersion::FIRST),
        )
        .data::<ModeCombatData>(&[])
        .data::<LeechData>(&[])
        .data::<CombatData>(&[])
        .data::<TurretData>(&[])
        .fields(
            ActionData::TABLE,
            ApiVersion::FIRST,
            StructFields::of::<AttackData>().iter().copied(),
        );
    }

    /// What combat adds to a `Unit` handle.
    fn register_unit(api: &mut ApiBuilder<'_>) {
        let recent_attackers = MemberDecl::method(
            ApiHolder::of::<Unit>(),
            "recent_attackers",
            &[&["ms"]],
            "the living units that struck it within the last `ms`, rounded up to whole ticks",
        );
        api.method(recent_attackers, |unit: Unit, ms: ScriptWindow| {
            CombatColumn::recent_attackers(&unit, ms)
        })
        .bind(
            MemberDecl::field(
                ApiHolder::of::<Unit>(),
                "attack_target",
                "the unit its attack aims at, `()` with none",
            ),
            |unit: &mut Unit| -> Checked<ScriptValue> {
                let view = unit.view();
                Ok(ActionsColumn::target(view, unit.admit(DataKind::Unit)?)
                    .and_then(|target| view.unit(target))
                    .map_or(ScriptValue::UNIT, ScriptValue::of))
            },
        )
        .bind(
            MemberDecl::field(
                ApiHolder::of::<Unit>(),
                "attack_range",
                "its attack's range",
            ),
            |unit: &mut Unit| -> Checked<Num> {
                ActionsColumn::attack_range(unit.view(), unit.admit(DataKind::Unit)?)
                    .ok_or_else(|| CombatRefusal::NoAttack.fail().into())
            },
        );
    }

    /// Brings back `unit`, which is dead and stays when dead, `ms` after this call, in ticks
    /// rounded up, at least one.
    fn respawn(ctx: &Ctx, unit: &Unit, ms: ScriptTicks) -> Checked<()> {
        if unit.read(|row| row.alive)? {
            return Err(CombatRefusal::RespawnAlive.fail().into());
        }
        if !CombatColumn::stays(unit)? {
            return Err(CombatRefusal::RespawnDespawns.fail().into());
        }
        ctx.queue(CombatEffect::Respawn {
            unit: unit.id,
            ticks: ms.get(),
        })
    }

    /// Queues an extra attack of the acting unit, which has an attack, on `target`.
    fn attack_hit(ctx: &Ctx, target: &Unit) -> Checked<()> {
        let view = ctx.view();
        let attacks = ctx
            .acting()
            .and_then(|id| view.row_index(id))
            .is_some_and(|row| ActionsColumn::attack_range(view, row).is_some());
        if !attacks {
            return Err(CombatRefusal::NoAttack.fail().into());
        }
        ctx.queue(CombatEffect::AttackHit { unit: target.id })
    }
}
