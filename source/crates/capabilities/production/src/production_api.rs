use campfire_actions::{ActionDataField, RequiresData};
use campfire_common::PlayerSlot;
use campfire_script::{
    ApiBuilder, ApiOwner, CapabilityRefusal, Checked, Ctx, DataTable, MemberSpec, Unit,
};
use campfire_sim::Capability;

use crate::drop_off_data::DropOffData;
use crate::error::ProductionRefusal;
use crate::mode_supply_data::ModeSupplyData;
use crate::node_data::NodeData;
use crate::production_column::ProductionColumn;
use crate::production_data::ProductionData;
use crate::supply_data::SupplyData;

/// What `production` gives scripts: each player's supply; and the data of `production` the
/// release runs: a train's unit type and requirements, a unit type's queue and supply, and the
/// mode's supply rules.
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
        api.action_fields(ActionDataField::of(Some(Capability::Production)))
            .data::<ProductionData>(&["queue", "exit", "rally"], &[])
            .data::<SupplyData>(&["cost", "provides"], &[])
            .shared_data(DataTable::Mode, &["supply"], &[])
            .shared_data(
                DataTable::UnitType,
                &["production", "supply", "node", "drop_off"],
                &[],
            )
            .data::<ModeSupplyData>(&["max"], &[])
            .data::<RequiresData>(&["units", "modifiers"], &[])
            .data::<NodeData>(&["resource", "amount"], &[])
            .data::<DropOffData>(&["resources"], &[])
            .bind(
                MemberSpec::field(ApiOwner::Unit, "load", "the amount it carries, 0 with none"),
                |unit: &mut Unit| ProductionColumn::load(unit),
            )
            .call(
                MemberSpec::call(
                    "supply_used",
                    &[&["player"]],
                    "what `player`'s living units and queued trains use of its supply",
                ),
                |ctx: &mut Ctx, player: PlayerSlot| Self::supply(ctx, player, SupplyCount::Used),
            )
            .call(
                MemberSpec::call(
                    "supply_cap",
                    &[&["player"]],
                    "what `player`'s living, complete units give of supply, at most the mode's `max`",
                ),
                |ctx: &mut Ctx, player: PlayerSlot| Self::supply(ctx, player, SupplyCount::Cap),
            );
    }

    /// `player`'s supply as the view read the match: the count `count` names.
    fn supply(ctx: &Ctx, player: PlayerSlot, count: SupplyCount) -> Checked<i64> {
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
