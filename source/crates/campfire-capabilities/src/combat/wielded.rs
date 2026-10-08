use crate::actions::purse::Purse;
use crate::actions::rank_values::RankValues;
use crate::actions::weapon::Weapon;
use crate::players::resource_amount::ResourceAmount;
use crate::units::action_id::ActionId;
use crate::units::block::Block;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;
use crate::values::rank::Rank;

/// The weapon of an attack under way: its slot, what it deals, and its values and its cost in
/// player resources at the slot's rank.
#[derive(Debug, Clone, Copy)]
pub(super) struct Wielded<'a> {
    pub(super) slot: u8,
    /// The weapon's action, which its damage names, and the rank of its slot.
    pub(super) action: ActionId,
    pub(super) rank: Rank,
    pub(super) weapon: Weapon,
    pub(super) values: RankValues,
    pub(super) resource_cost: &'a [ResourceAmount],
    /// The type of the homing projectile it fires, if it fires one.
    pub(super) projectile: Option<UnitType>,
}

impl Wielded<'_> {
    /// Whether its attack, going off, strikes for a unit with `tags`, under a forced move when
    /// `forced`: neither keeps it from attacking, and `purse` still affords its cost, as the
    /// checks run again at delivery.
    pub(super) fn strikes(&self, tags: Option<&UnitTags>, forced: bool, purse: Purse<'_>) -> bool {
        !UnitTags::blocks(tags, forced, Block::Attack)
            && purse.affords(&self.values.cost, self.resource_cost)
    }
}
