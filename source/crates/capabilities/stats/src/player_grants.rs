use bevy_ecs::change_detection::{DetectChanges, Ref};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Local, Query, Res, ResMut, SystemParam};
use campfire_common::Seat;
use campfire_players::{Player, PlayerRows};
use campfire_sim::{Removals, StableId};
use campfire_units::{Dead, Owner};
use campfire_values::{ModifierId, Rank, Relation, TagSet, UnitTags};

use crate::grants::{Grant, GrantFrom, Grants};
use crate::lifetime::Hold;
use crate::modifier_book::ModifierBook;
use crate::modifiers::Modifiers;
use crate::player_modifiers::PlayerModifiers;

/// The grants of the modifiers each unit holds from its player: each modifier its player holds
/// that the modifier's `affects` selects, from no source, at rank 1, while the unit lives.
#[derive(Debug)]
pub(crate) struct PlayerGrants;

/// The units whose player grants a run visits again though no part of theirs changed: those
/// that lost their owner, or came back to life, since the last run.
#[derive(SystemParam, Debug)]
pub(crate) struct Revisits<'w, 's> {
    unowned: Removals<'w, 's, Owner>,
    revived: Removals<'w, 's, Dead>,
}

/// The parts of a unit its player's grants read.
type PlayerUnit = (
    Entity,
    &'static StableId,
    Option<Ref<'static, Owner>>,
    Option<Ref<'static, UnitTags>>,
    Ref<'static, Modifiers>,
    Has<Dead>,
);

impl PlayerGrants {
    /// Pushes the grants of each unit's player modifiers: each one its player holds that its
    /// `affects` selects, at rank 1, while the unit lives, and none of each other it holds by its
    /// player. What a unit wants follows from its owner, its tags, its death, its modifiers, its
    /// player's modifiers and the book, so a run visits only the units of which one changed since
    /// its last, unless the book changed.
    pub(crate) fn want(
        book: Res<'_, ModifierBook>,
        (players, changed_players): (
            PlayerRows<'_, '_, PlayerModifiers>,
            Query<'_, '_, (&Player, Ref<'_, PlayerModifiers>)>,
        ),
        mut revisits: Revisits<'_, '_>,
        mut grants: ResMut<'_, Grants>,
        units: Query<'_, '_, PlayerUnit>,
        (mut changed, mut again, mut wanted): (
            Local<'_, Vec<Seat>>,
            Local<'_, Vec<Entity>>,
            Local<'_, Vec<ModifierId>>,
        ),
    ) {
        changed.clear();
        changed.extend(
            changed_players
                .iter()
                .filter(|(_, held)| held.is_changed())
                .map(|(player, _)| player.seat()),
        );
        again.clear();
        again.extend(revisits.unowned.read().chain(revisits.revived.read()));
        again.sort_unstable();
        let every = book.is_changed();
        for (entity, &id, owner, tags, modifiers, dead) in &units {
            let visits = every
                || owner
                    .as_ref()
                    .is_some_and(|owner| owner.is_changed() || changed.contains(&owner.seat()))
                || tags.as_ref().is_some_and(DetectChanges::is_changed)
                || modifiers.is_changed()
                || again.binary_search(&entity).is_ok();
            if !visits {
                continue;
            }
            wanted.clear();
            if !dead && let Some(held) = players.of(owner.as_deref()) {
                let tags = tags.map_or_else(TagSet::default, |tags| tags.tags);
                wanted.extend(held.iter().filter(|&modifier| {
                    let affects = book.get(modifier).spec.affects;
                    affects.is_none_or(|filter| filter.selects(Relation::Friendly, tags))
                }));
            }
            let held = modifiers.iter().filter(|carried| {
                let instance = carried.instance;
                instance.source.is_none() && instance.lifetime.held_by(Hold::Player)
            });
            let released = held
                .map(|carried| carried.instance.id)
                .filter(|modifier| !wanted.contains(modifier));
            let grant = |modifier, rank| Grant {
                modifier,
                from: GrantFrom::Nobody,
                hold: Hold::Player,
                action: None,
                rank,
            };
            let mut granted = wanted
                .iter()
                .map(|&modifier| grant(modifier, Some(Rank::FIRST)))
                .chain(released.map(|modifier| grant(modifier, None)))
                .peekable();
            if granted.peek().is_some() {
                grants.unit(entity, id);
                granted.for_each(|grant| grants.push(grant));
            }
        }
    }
}
