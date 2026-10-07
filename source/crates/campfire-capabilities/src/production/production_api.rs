use campfire_script::rhai::INT;
use campfire_sim::Capability;

use crate::actions::action_data_field::ActionDataField;
use crate::production::production_column::ProductionColumn;
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
            .data(DataTable::Production, &["queue"], &[])
            .data(DataTable::Supply, &["cost", "provides"], &[])
            .data(DataTable::Mode, &["supply"], &[])
            .data(DataTable::ModeSupply, &["max"], &[])
            .data(DataTable::Requires, &["units", "modifiers"], &[])
            .data(DataTable::Node, &["resource", "amount"], &[])
            .data(DataTable::DropOff, &["resources"], &[])
            .bind(
                MemberSpec::field(ApiOwner::Unit, "load", "the amount it carries, 0 with none")
                    .capability(Capability::Production),
                |unit: &mut Unit| ProductionColumn::load(unit),
            )
            .bind(
                MemberSpec::call(
                    "supply_used",
                    "(player)",
                    "what `player`'s living units and queued trains use of its supply",
                )
                .capability(Capability::Production),
                |ctx: &mut Ctx, player: INT| ProductionApi::supply(ctx, player, false),
            )
            .bind(
                MemberSpec::call(
                    "supply_cap",
                    "(player)",
                    "what `player`'s living, complete units give of supply, at most the mode's `max`",
                )
                .capability(Capability::Production),
                |ctx: &mut Ctx, player: INT| ProductionApi::supply(ctx, player, true),
            );
    }

    /// `player`'s supply as the view read the match: its cap when `cap`, else what it uses.
    fn supply(ctx: &Ctx, player: INT, cap: bool) -> Checked<INT> {
        let view = ctx.view();
        let slot = view.player(player)?;
        let supply =
            ProductionColumn::supply(view, slot).ok_or_else(|| ApiError::NoSupply.fail())?;
        let count = if cap { supply.cap } else { supply.used };
        Ok(INT::try_from(count).expect("a supply count fits an integer"))
    }
}
