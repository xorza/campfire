use campfire_math::Num;
use campfire_values::{Block, TagProperty};

use crate::scripts::api_version::ApiVersion;
use crate::scripts::ctx::{Ctx, Params};
use crate::scripts::data_table::DataTable;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::frame::Frame;
use crate::scripts::member_spec::MemberSpec;
use crate::scripts::script_api::api_builder::ApiBuilder;
use crate::scripts::script_call::ScriptCall;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::script_value::script_list::ScriptList;
use crate::scripts::status::Status;

/// The script API of the core: `ctx` itself, `ctx.p`, and the core's and the movement calls
/// the script API plans.
#[derive(Debug)]
pub struct CoreApi;

impl CoreApi {
    pub fn register(api: &mut ApiBuilder<'_>) {
        let chance = MemberSpec::call(
            "chance",
            &[&["p"]],
            "true with probability `p`, from 0 to 1, from the secret stream",
        );
        api.ty::<Ctx>("Ctx")
            .call(
                MemberSpec::value(
                    "p",
                    "the params: an ability's at its rank, a modifier's then its ability's, or the mode's",
                ),
                |ctx: &mut Ctx| Ok(Params(ctx.clone())),
            )
            .call(chance, |ctx: &mut Ctx, p: Num| Self::chance(ctx, p))
            .call(
                MemberSpec::call(
                    "pick",
                    &[&["list"]],
                    "an entry of `list`, each as likely, from the secret stream",
                ),
                |ctx: &mut Ctx, list: ScriptList| Self::pick(ctx, &list),
            )
            .shared_data(DataTable::UnitType, &["tags", "params", "state", "collision"], &[]);
        api.tag_property(
            TagProperty::Blocks(Block::Move),
            Status::Runs(ApiVersion::FIRST),
        )
        .tag_property(
            TagProperty::Blocks(Block::Use),
            Status::Runs(ApiVersion::FIRST),
        );
        api.ty::<Params>("Params")
            .index(|_, params: &mut Params, name: &str| params.get(name));
    }

    /// Draws whether a chance of `p` comes true, on the running call's sequence.
    fn chance(ctx: &Ctx, p: Num) -> Checked<bool> {
        if !(Num::ZERO..=Num::ONE).contains(&p) {
            return Err(ApiError::NotAProbability.fail().into());
        }
        let mut frame = ctx.write()?;
        Ok(Self::draws(&mut frame).chance(p))
    }

    /// Draws an entry of `list`, each as likely, on the running call's sequence.
    fn pick(ctx: &Ctx, list: &[ScriptValue]) -> Checked<ScriptValue> {
        if list.is_empty() {
            return Err(ApiError::EmptyPick.fail().into());
        }
        let mut frame = ctx.write()?;
        let at = Self::draws(&mut frame).pick(list.len());
        Ok(list[at].clone())
    }

    fn draws(frame: &mut Frame) -> &mut ScriptCall {
        frame
            .part_mut::<ScriptCall>()
            .expect("every match with scripts draws")
    }
}
