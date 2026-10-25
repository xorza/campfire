use bevy_ecs::change_detection::{DetectChanges, Ref};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_common::PlayerSlot;
use campfire_declarations::ById;
use campfire_players::{Player, PlayerRows};
use campfire_sim::StableId;
use campfire_units::{Dead, Owner};
use campfire_values::{ModifierId, Rank, TagSet, UnitTags, UnitType};

use crate::grants::{Grant, GrantFrom, Grants};
use crate::lifetime::Hold;
use crate::modifiers::Modifiers;
use crate::player_grants::Revisits;
use crate::player_modifiers::PlayerModifiers;
use crate::type_upgrades::TypeUpgrades;

/// The grants of the modifiers each unit holds by its type's upgrades: each entry's modifier
/// while its player's modifiers and its own meet the entry, from no source, at rank 1, while the
/// unit lives (upgrades.md, A unit type takes modifiers by upgrades).
#[derive(Debug)]
pub(crate) struct UpgradeGrants;

/// The parts of a unit its type's upgrades read.
type UpgradedUnit = (
    Entity,
    &'static StableId,
    Ref<'static, UnitType>,
    Option<Ref<'static, Owner>>,
    Option<Ref<'static, UnitTags>>,
    Ref<'static, Modifiers>,
    Has<Dead>,
);

impl UpgradeGrants {
    /// Pushes the grants of each unit's upgrades: each entry of its type that its player's
    /// modifiers and its own, those no player's or upgrade's hold gives it, meet, while it lives,
    /// and none of each other it holds by an upgrade. What a unit wants follows from its type, its
    /// owner, its tags, its death, its modifiers, its player's modifiers and the book, so a run
    /// visits only the units of which one changed since its last, unless the book changed, and of
    /// those only the units of a type with upgrades or that hold an upgrade's modifier.
    pub(crate) fn want(
        book: Res<'_, ById<UnitType, TypeUpgrades>>,
        (players, changed_players): (
            PlayerRows<'_, '_, PlayerModifiers>,
            Query<'_, '_, (&Player, Ref<'_, PlayerModifiers>)>,
        ),
        mut revisits: Revisits<'_, '_>,
        mut grants: ResMut<'_, Grants>,
        units: Query<'_, '_, UpgradedUnit>,
        (mut changed, mut again, mut wanted): (
            Local<'_, Vec<PlayerSlot>>,
            Local<'_, Vec<Entity>>,
            Local<'_, Vec<ModifierId>>,
        ),
    ) {
        changed.clear();
        changed.extend(
            changed_players
                .iter()
                .filter(|(_, held)| held.is_changed())
                .map(|(player, _)| player.player_slot()),
        );
        again.clear();
        again.extend(revisits.read());
        again.sort_unstable();
        let every = book.is_changed();
        for (entity, &id, unit_type, owner, tags, modifiers, dead) in &units {
            let upgrades = book.get(*unit_type);
            let visits = every
                || unit_type.is_changed()
                || owner.as_ref().is_some_and(|owner| {
                    owner.is_changed() || changed.contains(&owner.player_slot())
                })
                || tags.as_ref().is_some_and(DetectChanges::is_changed)
                || modifiers.is_changed()
                || again.binary_search(&entity).is_ok();
            let held = modifiers.iter().filter(|carried| {
                let instance = carried.instance;
                instance.source.is_none() && instance.lifetime.held_by(Hold::Upgrade)
            });
            if !visits || (upgrades.is_none() && held.clone().next().is_none()) {
                continue;
            }
            wanted.clear();
            let player = players.of(owner.as_deref());
            // Its own: what neither its player's hold nor an upgrade's gives it, as the player's
            // are read from the player, and an upgrade's modifier triggers no upgrade.
            let own = modifiers.iter().filter(|carried| {
                let instance = carried.instance;
                instance.source.is_some()
                    || !(instance.lifetime.held_by(Hold::Player)
                        || instance.lifetime.held_by(Hold::Upgrade))
            });
            let holds = |modifier| {
                player.is_some_and(|held| held.holds(modifier))
                    || own.clone().any(|carried| carried.instance.id == modifier)
            };
            let tags = tags.map_or(TagSet::NONE, |tags| tags.tags);
            if !dead {
                let entries = upgrades.into_iter().flat_map(|upgrades| &upgrades.0);
                let met = entries.filter(|entry| entry.met(tags, holds));
                for entry in met {
                    if !wanted.contains(&entry.modifier) {
                        wanted.push(entry.modifier);
                    }
                }
            }
            let released = held
                .map(|carried| carried.instance.id)
                .filter(|modifier| !wanted.contains(modifier));
            let grant = |modifier, rank| Grant {
                modifier,
                from: GrantFrom::Nobody,
                hold: Hold::Upgrade,
                action: None,
                rank,
            };
            let mut run = grants.of(entity, id);
            wanted
                .iter()
                .map(|&modifier| grant(modifier, Some(Rank::FIRST)))
                .chain(released.map(|modifier| grant(modifier, None)))
                .for_each(|grant| run.push(grant));
        }
    }
}
