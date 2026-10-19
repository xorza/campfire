use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_script::{
    Ctx, Hook, HookDecl, ModeAnswer, ModeEvents, PendingCalls, ScriptRole, ScriptValue, Unit,
};
use campfire_sim::{SimResource, StableId};
use serde::{Deserialize, Serialize};

/// The units trains made, in the order they spawned, whose `on_unit_trained` has yet to run. The
/// Mode stage runs the calls from the front; those whose call found the mode pool spent stay, and
/// run first in a later tick's Mode stage.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainedUnits(pub PendingCalls<Trained>);

/// A unit a train made, and the producer that trained it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trained {
    pub unit: StableId,
    pub producer: StableId,
}

impl SimResource for TrainedUnits {
    const NAME: &'static str = "production.trained_units";

    // A unit by any stable id, which the call finds or not.
    fn check(&self, _: &World) -> bool {
        true
    }
}

/// `on_unit_trained(ctx, unit, producer)`: a train made `unit`, which the mode's script answers.
#[derive(Debug)]
pub struct OnUnitTrained;

impl HookDecl for OnUnitTrained {
    const NAME: &'static str = "on_unit_trained";
    const ROLE: ScriptRole = ScriptRole::Mode;
    const PARAMS: &'static [&'static str] = &["ctx", "unit", "producer"];
    type Args = (Ctx, Unit, ScriptValue);
}

/// A unit a train made runs `on_unit_trained`, from the mode's pool, with the unit and its
/// producer, `()` once the producer is gone; one whose unit is gone runs no call.
impl ModeEvents for TrainedUnits {
    const HOOKS: &'static [Hook] = &[Hook::of::<OnUnitTrained>()];

    /// The units join as they spawn.
    fn arrive(_: &mut World) {}

    fn len(&self) -> usize {
        self.0.len()
    }

    fn partial(&self) -> u8 {
        self.0.partial()
    }

    fn answer(&self, answer: &mut ModeAnswer<'_, '_>) -> usize {
        self.0
            .iter()
            .take_while(|&&Trained { unit, producer }| {
                let args = |ctx: &Ctx| {
                    let view = ctx.view();
                    let unit = view.unit(unit)?;
                    let producer = view.unit(producer);
                    let producer = producer.map_or(ScriptValue::UNIT, ScriptValue::of);
                    Some((ctx.clone(), unit, producer))
                };
                answer.call::<OnUnitTrained>(Some(unit), args).is_continue()
            })
            .count()
    }

    fn answered(&mut self, count: usize, partial: u8) {
        self.0.answered(count, partial);
    }

    fn append(&mut self, later: &mut Self) {
        self.0.append(&mut later.0);
    }

    fn hold(&self, _: usize, _: &mut World) {}
}
