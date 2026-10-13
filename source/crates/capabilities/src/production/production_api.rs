use campfire_sim::Capability;

use crate::actions::action_data_field::ActionDataField;
use crate::actions::requires_data::RequiresData;
use crate::production::drop_off_data::DropOffData;
use crate::production::mode_supply_data::ModeSupplyData;
use crate::production::node_data::NodeData;
use crate::production::production_column::ProductionColumn;
use crate::production::production_data::ProductionData;
use crate::production::supply_data::SupplyData;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::units::unit::Unit;

/// What `production` gives scripts: each player's supply; and the data of `production` the
/// release runs: a train's unit type and requirements, a unit type's queue and supply, and the
/// mode's supply rules.
#[derive(Debug)]
pub(crate) struct ProductionApi;

impl ProductionApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.action_fields(ActionDataField::of(Some(Capability::Production)))
            .data::<ProductionData>(&["queue"], &[])
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
                |ctx: &mut Ctx, player: i64| ProductionApi::supply(ctx, player, false),
            )
            .call(
                MemberSpec::call(
                    "supply_cap",
                    &[&["player"]],
                    "what `player`'s living, complete units give of supply, at most the mode's `max`",
                ),
                |ctx: &mut Ctx, player: i64| ProductionApi::supply(ctx, player, true),
            );
    }

    /// `player`'s supply as the view read the match: its cap when `cap`, else what it uses.
    fn supply(ctx: &Ctx, player: i64, cap: bool) -> Checked<i64> {
        let view = ctx.view();
        let slot = view.names().player(player)?;
        let supply =
            ProductionColumn::supply(view, slot).ok_or_else(|| ApiError::NoSupply.fail())?;
        let count = if cap { supply.cap } else { supply.used };
        Ok(i64::try_from(count).expect("a supply count fits an integer"))
    }
}
