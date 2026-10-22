use campfire_math::Num;
use campfire_values::{Block, TagProperty};

use crate::scripts::api_holder::ApiHolder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::ctx::{Ctx, Params};
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::frame::Frame;
use crate::scripts::member_decl::MemberDecl;
use crate::scripts::player_handle::PlayerHandle;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::api_builder::ApiBuilder;
use crate::scripts::script_call::ScriptCall;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::script_value::script_list::ScriptList;
use crate::scripts::status::Status;

/// The script API of the core: `ctx` itself, `ctx.params`, the `Player` handle's slot and
/// comparisons, and the core's and the movement calls the script API plans.
#[derive(Debug)]
pub struct CoreApi;

impl CoreApi {
    pub fn register(api: &mut ApiBuilder<'_>) {
        let chance = MemberDecl::call(
            "chance",
            &[&["p"]],
            "true with probability `p`, from 0 to 1, from the secret stream",
        );
        api.ty::<Ctx>()
            .call(
                MemberDecl::value(
                    "params",
                    "the params: an ability's at its rank, a modifier's then its ability's, the mode's, or a bot's then the mode's",
                )
                .roles(RoleSet::SIM.and(RoleSet::BOT)),
                |ctx: &mut Ctx| Ok(Params(ctx.clone())),
            )
            .call(chance, |ctx: &mut Ctx, p: Num| Self::chance(ctx, p))
            .call(
                MemberDecl::call(
                    "pick",
                    &[&["list"]],
                    "an entry of `list`, each as likely, from the secret stream",
                ),
                |ctx: &mut Ctx, list: ScriptList| Self::pick(ctx, &list),
            );
        api.tag_property(
            TagProperty::Blocks(Block::Move),
            Status::Runs(ApiVersion::FIRST),
        )
        .tag_property(
            TagProperty::Blocks(Block::Use),
            Status::Runs(ApiVersion::FIRST),
        );
        api.ty::<Params>()
            .index(|_, params: &mut Params, name: &str| params.get(name));
        let player = ApiHolder::of::<PlayerHandle>();
        api.ty::<PlayerHandle>()
            .bind(
                MemberDecl::field(player, "seat", "its seat's place, from 0"),
                |player: &mut PlayerHandle| i64::from(player.seat().get()),
            )
            .method(
                MemberDecl::operator(player, "==", "whether the two are one player"),
                |a: PlayerHandle, b: PlayerHandle| Ok(a == b),
            )
            .method(
                MemberDecl::operator(player, "!=", "whether the two are two players"),
                |a: PlayerHandle, b: PlayerHandle| Ok(a != b),
            );
    }

    /// Draws whether a chance of `p` comes true, on the running call's sequence.
    fn chance(ctx: &Ctx, p: Num) -> Checked<bool> {
        if !(Num::ZERO..=Num::ONE).contains(&p) {
            return Err(ApiError::NotAProbability.fail().into());
        }
        let mut frame = ctx.drawing()?;
        Ok(Self::draws(&mut frame).chance(p))
    }

    /// Draws an entry of `list`, each as likely, on the running call's sequence.
    fn pick(ctx: &Ctx, list: &[ScriptValue]) -> Checked<ScriptValue> {
        if list.is_empty() {
            return Err(ApiError::EmptyPick.fail().into());
        }
        let mut frame = ctx.drawing()?;
        let at = Self::draws(&mut frame).pick(list.len());
        Ok(list[at].clone())
    }

    fn draws(frame: &mut Frame) -> &mut ScriptCall {
        frame
            .extension_mut::<ScriptCall>()
            .expect("every match with scripts draws")
    }
}
