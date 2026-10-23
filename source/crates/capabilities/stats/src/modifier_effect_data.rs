use std::sync::Arc;

use campfire_actions::{
    Amount, EffectError, EffectResolveCx, EffectRuleCx, LaunchLists, ListedAt, ListedEffect,
    ListedEffects, ListedSpec,
};
use campfire_common::DeclaredName;
use campfire_common::Ticks;
use campfire_declarations::{BookError, Listed};
use campfire_script::{
    ApiBuilder, Applies, CallError, Checked, Ctx, Frame, MemberDecl, ScriptTicks, Unit, View,
};
use campfire_sim::Capability;
use campfire_values::{ModifierId, Number};
use serde::Deserialize;

use crate::modifier_handle::ModifierHandle;
use crate::stats_call::StatsCall;
use crate::stats_column::StatsColumn;
use crate::stats_effect::StatsEffect;

/// `modifier = { id, duration_ms }`: a modifier of the action's package, from the acting unit.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModifierEffectData {
    pub id: DeclaredName,
    pub duration_ms: Option<Number>,
}

/// A listed modifier, its names resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ModifierEffectSpec {
    id: ModifierId,
    duration_ms: Option<Amount>,
}

impl Listed for ModifierEffectData {
    type Set = ListedEffects;

    const KEY: &'static str = "modifier";

    fn into_listed(self) -> Arc<dyn ListedEffect> {
        Arc::new(self)
    }

    fn api(api: &mut ApiBuilder<'_>) {
        let add = MemberDecl::call(
            "add_modifier",
            &[&["unit", "id"], &["unit", "id", "duration_ms"]],
            "applies the modifier `id` of the script's package to `unit` from the acting unit, with \
             the call's action at its rank, which gives each param the modifier reads and does not \
             declare, and returns its handle",
        )
        .applies(Applies::WithAction);
        api.call(add, |ctx: &mut Ctx, target: Unit, id: ModifierId| {
            add_modifier(ctx, &target, id, None)
        })
        .call(
            add,
            |ctx: &mut Ctx, target: Unit, id: ModifierId, ms: ScriptTicks| {
                add_modifier(ctx, &target, id, Some(ms.get()))
            },
        );
    }
}

/// Queues modifier `id` of the call's package on `target`, from the acting unit, for `duration`
/// when given, and gives its handle; an error when the call's action at its rank does not give a
/// param `id` reads.
fn add_modifier(
    ctx: &Ctx,
    target: &Unit,
    id: ModifierId,
    duration: Option<Ticks>,
) -> Checked<ModifierHandle> {
    let mut frame = ctx.write()?;
    let (source, action, rank) = (frame.acting(), frame.action(), frame.rank());
    let call = StatsCall::of_mut(&mut frame);
    StatsColumn::check_way(ctx.view(), call, id, action, rank)?;
    let handle = StatsColumn::applied_handle(ctx.view(), call, target.id, id, source);
    frame.queue(StatsEffect::Add {
        target: target.id,
        id,
        duration,
    });
    Ok(handle)
}

impl ListedEffect for ModifierEffectData {
    fn needs(&self) -> Option<Capability> {
        Some(Capability::Stats)
    }

    fn numbers(&self) -> Vec<&Number> {
        self.duration_ms.iter().collect()
    }

    fn duration(&self) -> Option<&Number> {
        self.duration_ms.as_ref()
    }

    fn modifier(&self) -> Option<&DeclaredName> {
        Some(&self.id)
    }

    fn lists(&self) -> Option<&LaunchLists> {
        None
    }

    fn takes_point(&self) -> bool {
        false
    }

    fn rule(&self, _: &dyn EffectRuleCx) -> Result<(), BookError> {
        Ok(())
    }

    fn resolve(&self, cx: &mut dyn EffectResolveCx) -> Result<Arc<dyn ListedSpec>, EffectError> {
        Ok(Arc::new(ModifierEffectSpec {
            id: cx
                .names()
                .named::<ModifierId>((cx.names().package(), self.id.as_str()))?,
            duration_ms: cx.amount_if(self.duration_ms.as_ref())?,
        }))
    }
}

impl ListedSpec for ModifierEffectSpec {
    fn queue(&self, at: ListedAt, frame: &mut Frame, view: &View) -> Result<(), CallError> {
        let duration = self.duration_ms.map(|ms| ms.duration(frame, view.rate()));
        frame.queue(StatsEffect::Add {
            target: at.unit(frame),
            id: self.id,
            duration,
        });
        Ok(())
    }
}
