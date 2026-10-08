use campfire_math::Num;
use campfire_script::rhai::{Array, Dynamic, INT};

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::ctx::{Ctx, Params};
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::frame::Frame;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_api::status::Status;
use crate::scripts::scripts_call::ScriptsCall;
use crate::units::block::Block;
use crate::units::tag_property::TagProperty;

/// The script API of the core: `ctx` itself, `ctx.p`, and the core's and the movement calls
/// design 08 plans.
#[derive(Debug)]
pub(crate) struct CoreApi;

impl CoreApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let chance = MemberSpec::call(
            "chance",
            &[&["p"]],
            "true with probability `p`, from 0 to 1, from the secret stream",
        );
        api.ty::<Ctx>("Ctx")
            .bind(
                MemberSpec::value(
                    "p",
                    "the params: an ability's at its rank, a modifier's then its ability's, or the mode's",
                ),
                |ctx: &mut Ctx| Params(ctx.clone()),
            )
            .bind(chance, |ctx: &mut Ctx, p: Num| CoreApi::chance(ctx, p))
            .bind(chance, |ctx: &mut Ctx, p: INT| {
                CoreApi::chance(ctx, ApiError::num(p)?)
            })
            .bind(
                MemberSpec::call(
                    "pick",
                    &[&["list"]],
                    "an entry of `list`, each as likely, from the secret stream",
                ),
                |ctx: &mut Ctx, list: Array| CoreApi::pick(ctx, &list),
            )
            .data(DataTable::ModeNavigation, &["layers"], &[])
            .data(DataTable::Collision, &["radius", "box", "layer"], &[])
            .data(DataTable::Tag, &["blocks", "hidden", "detects", "immune"], &[]);
        api.tag_property(
            TagProperty::Blocks(Block::Move),
            Status::Runs(ApiVersion::FIRST),
        )
        .tag_property(
            TagProperty::Blocks(Block::Use),
            Status::Runs(ApiVersion::FIRST),
        );
        api.ty::<Params>("Params")
            .index(|params: &mut Params, name: &str| params.get(name));
    }

    /// Draws whether a chance of `p` comes true, on the running call's sequence.
    fn chance(ctx: &Ctx, p: Num) -> Checked<bool> {
        if !(Num::ZERO..=Num::ONE).contains(&p) {
            return Err(ApiError::NotAProbability.fail().into());
        }
        let mut frame = ctx.write()?;
        Ok(CoreApi::draws(&mut frame).chance(p))
    }

    /// Draws an entry of `list`, each as likely, on the running call's sequence.
    fn pick(ctx: &Ctx, list: &Array) -> Checked<Dynamic> {
        if list.is_empty() {
            return Err(ApiError::EmptyPick.fail().into());
        }
        let mut frame = ctx.write()?;
        let at = CoreApi::draws(&mut frame).pick(list.len());
        Ok(list[at].clone())
    }

    fn draws(frame: &mut Frame) -> &mut ScriptsCall {
        frame
            .part_mut::<ScriptsCall>()
            .expect("every match with scripts draws")
    }
}
