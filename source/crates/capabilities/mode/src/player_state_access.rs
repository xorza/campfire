use campfire_common::PlayerSlot;
use campfire_script::{ApiBuilder, ApiError, Checked, Ctx, ScriptHandle, ScriptValue};

use crate::mode_call::ModeCall;
use crate::mode_ctx::ModeCtx;
use crate::player_state;

/// `player.state`, `PlayerState` in scripts: the script state fields of a player a script holds,
/// by name, to read and write, as its mode's `[player_state]` declares them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlayerStateAccess(pub(crate) PlayerSlot);

impl PlayerStateAccess {
    /// Indexes `PlayerState` by field name, to read and to write.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.ty::<Self>()
            .index(|args, state: &mut Self, name| state.get(args.get(), name))
            .index_set(|args, state: &mut Self, name, value| state.set(args.get(), name, &value));
    }

    /// The field `name`, as the call last wrote it, or as the batch holds it; one the mode does
    /// not declare fails the call, and so does one of an audience the view's viewer is not of.
    fn get(self, ctx: &Ctx, name: &str) -> Checked<ScriptValue> {
        let book = ctx.mode_spec_or_fail()?;
        let field = book.schema.players.field(name);
        let field = field.ok_or_else(|| ApiError::UnknownState.fail())?;
        let audience = book.schema.players.place(field).audience;
        ctx.view()
            .admit_player(self.0, player_state::kind_of(audience))?;
        let frame = ctx.frame();
        let value = ModeCall::of(&frame).players.get(self.0, field);
        Ok(book.schema.players.read(field, value, ctx.view()))
    }

    /// Writes `value` to the field `name`, for the call to read back and to apply when it ends;
    /// a field the mode does not declare, a value no value of the field, and a call that may not
    /// write fail the call.
    fn set(self, ctx: &Ctx, name: &str, value: &ScriptValue) -> Checked<()> {
        let book = ctx.mode_spec_or_fail()?;
        let schema = &book.schema.players;
        let field = schema
            .field(name)
            .ok_or_else(|| ApiError::UnknownState.fail())?;
        let value = schema.take(field, value)?;
        ModeCall::of_mut(&mut *ctx.write()?)
            .players
            .set(self.0, field, value);
        Ok(())
    }
}

impl ScriptHandle for PlayerStateAccess {
    const NAME: &'static str = "PlayerState";
    const TITLE: &'static str = "A player's state";
}
