use campfire_sim::Capability;

use crate::actions::action_data_field::ActionDataField;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::hook::Hook;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_api::status::Status;
use crate::units::block::Block;
use crate::units::tag_effect::TagEffect;

/// The script API of `abilities`: the values of a cast that design 08 plans, and the
/// bookkeeping of cooldowns and charges.
#[derive(Debug)]
pub(crate) struct AbilitiesApi;

impl AbilitiesApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let cast = |name, description| {
            MemberSpec::value(name, description)
                .roles(RoleSet::ACTION)
                .capability(Capability::Abilities)
        };
        let call = |name, signature, description| {
            MemberSpec::call(name, signature, description).capability(Capability::Abilities)
        };
        api.plan(cast("range", "the ability's range at its rank"))
            .plan(cast(
                "charge",
                "how long a charged cast was held, from 0 to 1",
            ))
            .plan(cast("origin", "where the cast comes from"))
            .plan(call(
                "reduce_cooldown",
                "(unit, id, ms)",
                "takes `ms` off the cooldown of `unit`'s ability `id`",
            ))
            .plan(call(
                "reduce_cooldowns",
                "(unit, fraction)",
                "takes `fraction` off the cooldowns of `unit`'s basic abilities",
            ))
            .plan(call(
                "add_charge",
                "(unit, id)",
                "gives `unit`'s ability `id` a charge",
            ));
        api.tag_effect(
            TagEffect::Blocks(Block::Cast),
            Status::Runs(ApiVersion::FIRST),
        )
        .hook(Hook::OnResolve, Status::Runs(ApiVersion::FIRST))
        .hook(Hook::OnChannelTick, Status::Planned)
        .hook(Hook::OnInterrupt, Status::Planned)
        .action_fields(ActionDataField::of(Some(Capability::Abilities)))
        .data(
            DataTable::Effect,
            &["damage", "heal", "restore", "modifier", "xp", "to"],
            &["purge", "spawn", "launch", "move", "loot", "noise"],
        );
    }
}
