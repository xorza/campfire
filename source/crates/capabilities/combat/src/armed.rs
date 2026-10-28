use std::cmp::Reverse;

use campfire_actions::{ActionBook, ActionSlots, SlotTimes};
use campfire_common::Tick;
use campfire_declarations::ById;
use campfire_math::Num;
use campfire_values::{ActionId, Relation, TagSet};

use crate::armor::Armor;
use crate::weapon::Weapon;

/// What a unit chooses its weapon by: its slots and their times, the action and weapon books, its
/// tags and its stats, in the tick `now`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Armed<'a> {
    pub(crate) slots: &'a ActionSlots,
    pub(crate) times: Option<&'a SlotTimes>,
    pub(crate) book: &'a ActionBook,
    pub(crate) weapons: &'a ById<ActionId, Weapon>,
    pub(crate) holder: TagSet,
    pub(crate) stats: &'a [Num],
    pub(crate) now: Tick,
}

impl Armed<'_> {
    /// The slot of the weapon it attacks a target of `tags` with, a unit it regards with
    /// `relation`, of the armor `armor`, each weapon's reach of it as `reaches` says of its slot:
    /// of its weapons whose `when` its tags pass, whose filter selects the target and whose kind
    /// the armor does not spare, of those that reach the target one ready, or preferred, before one
    /// cooling, then one preferred, then the one of the most estimated damage, then the lowest
    /// slot; and while none reaches it, the first, as Zero Hour's
    /// `WeaponSet::chooseBestWeaponForTarget` walks its slots and falls back to its primary
    /// (04-capabilities/combat.md, Combat chooses the weapon; 04-capabilities/combat.md, An attack
    /// may prefer its targets). None for no such weapon.
    pub(crate) fn choose(
        self,
        (relation, tags): (Relation, TagSet),
        armor: Option<&Armor>,
        reaches: impl Fn(u8) -> bool,
    ) -> Option<u8> {
        let mut weapons = self
            .slots
            .weapons_in(self.book, self.holder, Some((relation, tags)))
            .filter_map(|at| {
                let slot = self.slots.slot(at).expect("a weapon's slot");
                let rank = slot.rank.expect("a weapon is learned");
                let action = slot.action.expect("a weapon's slot holds its action");
                let weapon = self.weapons.get(action).expect("an attack is a weapon");
                let spared = armor.is_some_and(|armor| armor.spares(weapon.kind));
                (!spared).then_some((at, rank, weapon))
            })
            .peekable();
        let first = weapons.peek().map(|&(at, ..)| at);
        let ranked = weapons
            .filter(|&(at, ..)| reaches(at))
            .map(|(at, rank, weapon)| {
                let ready = self
                    .times
                    .and_then(|times| times.slot(at))
                    .is_none_or(|time| time.ready_at <= self.now);
                let preferred = weapon.prefers(tags);
                let estimate = weapon.estimate(self.stats, rank, armor);
                ((ready || preferred, preferred, estimate, Reverse(at)), at)
            });
        ranked
            .max_by_key(|&(rank, _)| rank)
            .map(|(_, at)| at)
            .or(first)
    }
}
