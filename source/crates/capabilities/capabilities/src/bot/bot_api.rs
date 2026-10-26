use campfire_codec::TomlValue;
use campfire_common::PlayerSlot;
use campfire_script::{
    ApiBuilder, ApiError, Checked, Ctx, HookDecl, MemberDecl, PlayerHandle, RoleSet, ScriptList,
    ScriptMap, ScriptRole, ScriptText, ScriptValue, Unit,
};
use campfire_sim::Capability;
use campfire_values::UnitType;

use crate::bot::bot_call::{BotCall, ThinkOrder};
use crate::order::Directive;
use crate::order::named_refs::NamedRefs;

/// The script API of a bot's think, which only a bot's calls use: its own player, its own units,
/// and its orders; a bot reads the rest through the calls the sim's roles share with it
/// (03-game-scripting.md, A bot is a Rhai script of its game, which the server's bot driver runs).
#[derive(Debug)]
pub(crate) struct BotApi;

/// `on_bot_think(ctx)`: a bot's think, which its driver calls every `think_ms` of its bot.
#[derive(Debug)]
pub struct OnBotThink;

impl HookDecl for OnBotThink {
    const NAME: &'static str = "on_bot_think";
    const ROLE: ScriptRole = ScriptRole::Bot;
    const PARAMS: &'static [&'static str] = &["ctx"];
    type Args = (Ctx,);
}

impl BotApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let bot = |decl: MemberDecl| decl.roles(RoleSet::BOT).capability(Capability::Orders);
        api.call(
            bot(MemberDecl::value("player", "the bot's own player")),
            |ctx: &mut Ctx| Ok(PlayerHandle::new(Self::player_slot(ctx)?)),
        )
        .call(
            bot(MemberDecl::call(
                "own",
                &[&["type"]],
                "the bot's own living units of `type`, by stable id, its sites among them",
            )),
            |ctx: &mut Ctx, unit_type: UnitType| -> Checked<ScriptList> {
                let player_slot = Self::player_slot(ctx)?;
                Ok(ctx.view().units_where(|row| {
                    row.alive && row.owner == Some(player_slot) && row.unit_type == unit_type
                }))
            },
        )
        .call(
            bot(MemberDecl::call(
                "may_start",
                &[&["unit", "action"]],
                "whether `unit`, the bot's own, may start the train or the build of its slot \
                     that holds the action `action`, of any package, as the think began, as the \
                     order's own checks find it; a build's place its order finds",
            )),
            |ctx: &mut Ctx, unit: Unit, action: ScriptText| Self::may_start(ctx, &unit, &action),
        )
        .call(
            bot(MemberDecl::call(
                "idle",
                &[&["unit"]],
                "whether `unit`, the bot's own, stood idle as the think began: no action \
                     under way, no target, nowhere to walk, no gather and no build",
            )),
            |ctx: &mut Ctx, unit: Unit| Self::idle(ctx, &unit),
        )
        .call(
            bot(MemberDecl::call(
                "order",
                &[&["units", "directive"]],
                "gives `units`, a list of the bot's own, the order `directive`, a table as an \
                     order script's `directive` writes it, once the think returns",
            )),
            |ctx: &mut Ctx, units: ScriptList, directive: ScriptMap| -> Checked<()> {
                Self::order(ctx, &units, directive)
            },
        );
    }

    /// Whether the bot's own `unit` may start the action named `action`; one not its own fails.
    fn may_start(ctx: &Ctx, unit: &Unit, action: &ScriptText) -> Checked<bool> {
        Self::own(ctx, unit)?;
        let names = ctx.view().names();
        let frame = ctx.frame();
        let call = frame
            .extension::<BotCall>()
            .expect("a bot's think runs in a frame of its call");
        let mut startable = call.startable(unit.id);
        Ok(startable.any(|held| ScriptText::from(names.name(held)) == *action))
    }

    /// Whether the bot's own `unit` stood idle; one not its own fails.
    fn idle(ctx: &Ctx, unit: &Unit) -> Checked<bool> {
        Self::own(ctx, unit)?;
        let frame = ctx.frame();
        let call = frame
            .extension::<BotCall>()
            .expect("a bot's think runs in a frame of its call");
        Ok(call.idle(unit.id))
    }

    /// Fails unless `unit` is the bot's own.
    fn own(ctx: &Ctx, unit: &Unit) -> Checked<()> {
        let player_slot = Self::player_slot(ctx)?;
        if unit.read(|row| row.owner)? != Some(player_slot) {
            return Err(ApiError::NotForRole.fail().into());
        }
        Ok(())
    }

    /// The player slot of the bot whose think runs.
    fn player_slot(ctx: &Ctx) -> Checked<PlayerSlot> {
        let payer = ctx.frame().payer();
        payer.ok_or_else(|| ApiError::NotForRole.fail().into())
    }

    /// Queues the order `directive` to `units`; a list with a value that is no unit, a unit that
    /// is not the bot's own, as `may_start` and `idle` refuse one, or a table that reads as no
    /// directive, fails the call.
    fn order(ctx: &Ctx, units: &ScriptList, directive: ScriptMap) -> Checked<()> {
        let units = units
            .iter()
            .map(|value| value.clone().try_cast::<Unit>())
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| ApiError::NotAUnit.fail())?;
        for unit in &units {
            Self::own(ctx, unit)?;
        }
        let units = units.iter().map(|unit| unit.id).collect();
        let value = ScriptValue::of(directive);
        let directive = TomlValue::try_of(&value)
            .ok()
            .and_then(|value| value.read::<Directive<NamedRefs>>().ok())
            .ok_or_else(|| ApiError::NotADirective.fail())?;
        let mut frame = ctx.frame();
        let call = frame
            .extension_mut::<BotCall>()
            .expect("a bot's think runs in a frame of its call");
        call.give(ThinkOrder { units, directive });
        Ok(())
    }
}
