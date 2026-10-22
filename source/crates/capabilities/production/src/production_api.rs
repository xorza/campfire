use campfire_actions::ActionData;
use campfire_codec::StructFields;
use campfire_common::Seat;
use campfire_script::{
    ApiBuilder, ApiHolder, ApiVersion, CapabilityRefusal, Checked, Ctx, MemberDecl, PlayerHandle,
    Status, Unit,
};
use campfire_values::{Block, TagProperty};

use crate::build_data::BuildData;
use crate::drop_off_data::DropOffData;
use crate::error::ProductionRefusal;
use crate::gather_data::GatherData;
use crate::mode_supply_data::ModeSupplyData;
use crate::node_data::NodeData;
use crate::production_column::ProductionColumn;
use crate::production_data::ProductionData;
use crate::requires_data::RequiresData;
use crate::supply_data::SupplyData;
use crate::train_data::TrainData;

/// What `production` gives scripts: what a unit carries, builds, trains and holds, and each
/// player's supply; and the data of `production` the release runs: a train's unit type and
/// requirements, a unit type's queue and supply, and the mode's supply rules.
#[derive(Debug)]
pub struct ProductionApi;

/// A count of a player's supply: what its units and trains use, or its cap.
#[derive(Debug, Clone, Copy)]
enum SupplyCount {
    Used,
    Cap,
}

impl ProductionApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let mut kinds: Vec<&'static str> = Vec::new();
        let sections = [
            StructFields::of::<TrainData>(),
            StructFields::of::<BuildData>(),
            StructFields::of::<GatherData>(),
        ];
        for field in sections.into_iter().flatten() {
            if !kinds.contains(field) {
                kinds.push(field);
            }
        }
        api.fields(ActionData::TABLE, ApiVersion::FIRST, kinds)
            .data::<ProductionData>(&[])
            .data::<SupplyData>(&[])
            .data::<ModeSupplyData>(&[])
            .data::<RequiresData>(&[])
            .tag_property(
                TagProperty::Blocks(Block::Produce),
                Status::Runs(ApiVersion::FIRST),
            )
            .data::<NodeData>(&[])
            .data::<DropOffData>(&[])
            .bind(
                MemberDecl::field(
                    ApiHolder::of::<Unit>(),
                    "load",
                    "the amount it carries, 0 with none",
                ),
                |unit: &mut Unit| ProductionColumn::load(unit),
            )
            .bind(
                MemberDecl::field(
                    ApiHolder::of::<Unit>(),
                    "site_progress",
                    "the share of its build's time its site reached, from 0 to 1, `()` when it is \
                     no site",
                ),
                |unit: &mut Unit| ProductionColumn::site_progress(unit),
            )
            .bind(
                MemberDecl::field(
                    ApiHolder::of::<Unit>(),
                    "queue",
                    "the unit types its queue trains, first the one under way, none with no queue",
                ),
                |unit: &mut Unit| ProductionColumn::queue(unit),
            )
            .bind(
                MemberDecl::field(
                    ApiHolder::of::<Unit>(),
                    "node_amount",
                    "the amount it holds as a node, `()` when it is no node",
                ),
                |unit: &mut Unit| ProductionColumn::node_amount(unit),
            )
            .bind(
                MemberDecl::field(
                    ApiHolder::of::<PlayerHandle>(),
                    "supply_used",
                    "what its living units and queued trains use of its supply",
                ),
                |ctx: &Ctx, player: &mut PlayerHandle| {
                    Self::supply(ctx, player.seat(), SupplyCount::Used)
                },
            )
            .bind(
                MemberDecl::field(
                    ApiHolder::of::<PlayerHandle>(),
                    "supply_cap",
                    "what its living, complete units give of supply, at most the mode's `max`",
                ),
                |ctx: &Ctx, player: &mut PlayerHandle| {
                    Self::supply(ctx, player.seat(), SupplyCount::Cap)
                },
            );
    }

    /// `player`'s supply as the view read the match: the count `count` names.
    fn supply(ctx: &Ctx, player: Seat, count: SupplyCount) -> Checked<i64> {
        ctx.admit_seat(Some(player))?;
        let view = ctx.view();
        let supply = ProductionColumn::supply(view, player)
            .ok_or_else(|| ProductionRefusal::NoSupply.fail())?;
        let count = match count {
            SupplyCount::Used => supply.used,
            SupplyCount::Cap => supply.cap,
        };
        Ok(i64::try_from(count).expect("a supply count fits an integer"))
    }
}
