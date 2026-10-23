use bevy_ecs::resource::Resource;
use bevy_ecs::system::Local;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_declarations::{Book, BookCx, BookError, BookPlace, ById, ByIdBuilder, Section};
use campfire_math::Num;
use campfire_players::{PlayersWorld, ResourceBook};
use campfire_script::{Filter, MatchNames, UnknownName};
use campfire_sim::{Capability, EntityIndex};
use campfire_units::{Deaths, Owner, UnitTypes, UnitsWorld};
use campfire_values::{Named, PackageIndex, Relations, ResourceId, Team, UnitTags, UnitType};

use crate::bounty_data::{BountyData, ModeBountyData, Pays};
use crate::bounty_share::BountyShare;
use crate::error::BountyError;

/// The mode's kill bounty, as its `[bounty]` and its unit types' give it; none for a mode of no
/// `[bounty]`. Package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct BountyBook(Option<Bounty>);

/// The killers whose kills pay, the share each player starts with, and what each type's death
/// pays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bounty {
    killer: Filter,
    victim: Filter,
    start: Num,
    types: ById<UnitType, TypeBounty>,
}

/// What a type's death pays, in its resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TypeBounty {
    resource: ResourceId,
    pays: Pays,
}

impl BountyBook {
    /// The bounty of the mode's `[bounty]` and its types', their names resolved; an error for a
    /// filter's tag or a resource the match does not have, for a type's bounty in a mode of none,
    /// and for a bounty in a match without combat, whose deaths it pays.
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let mode = cx.mode_section::<ModeBountyData>();
        if mode.is_some() {
            cx.require(Capability::Combat, PackageIndex::MODE, || {
                BookPlace::Section(ModeBountyData::KEY)
            })?;
        }
        let resources = cx.book::<ResourceBook>()?;
        let mut types = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<BountyData>() else {
                continue;
            };
            let at = BookPlace::UnitType(unit.place());
            if mode.is_none() {
                return Err(BookError::rule(unit.package, at, BountyError::NoModeBounty));
            }
            let resource = resources.0.named(data.resource.as_str()).ok_or_else(|| {
                let name = UnknownName::new(ResourceId::NAMES, &data.resource);
                BookError::unknown(unit.package, at, name)
            })?;
            let pays = data.pays;
            types.set(unit.unit_type, TypeBounty { resource, pays });
        }
        let Some(mode) = mode else {
            return Ok(Self(None));
        };
        let tags = cx.fact::<UnitTypes>()?.tags();
        let resolve = |filter| {
            Filter::resolve(filter, tags).map_err(|name| {
                let at = BookPlace::Section(ModeBountyData::KEY);
                BookError::unknown(PackageIndex::MODE, at, name)
            })
        };
        Ok(Self(Some(Bounty {
            killer: resolve(&mode.killer)?,
            victim: resolve(mode.victim.filter())?,
            start: mode.share,
            types: types.build(),
        })))
    }

    /// The share each player starts with, of a mode of a bounty.
    pub(crate) fn start(&self) -> Option<Num> {
        self.0.as_ref().map(|bounty| bounty.start)
    }

    pub(crate) const fn bounty(&self) -> Option<&Bounty> {
        self.0.as_ref()
    }
}

impl Book for BountyBook {
    const SCRIPTS_ONLY: bool = true;

    fn share(&self, _: &mut MatchNames) {}
}

impl Bounty {
    /// Pays the tick's deaths' bounties: of a victim of a type of a bounty whose tags at its death
    /// the mode's victim filter selects, and a killer that passes the mode's filter from the
    /// victim's side, the killer's owner takes the type's amount,
    /// or its share of the type's cost, rounded up, of the type's resource. A killer gone, or of
    /// no owner, takes none, and a payment past what a resource holds is not made, as a script's
    /// `add_resource` fails (04-capabilities/games.md, A kill's bounty).
    pub(crate) fn pay(world: &mut World, mut payments: Local<'_, Vec<Payment>>) {
        payments.clear();
        Self::payments(world, &mut payments);
        for &Payment {
            player_slot,
            resource,
            amount,
        } in &*payments
        {
            if let Some(mut resources) = world.player_resources_mut(player_slot) {
                resources.gain(resource, amount);
            }
        }
    }

    /// Fills `payments` with what the tick's deaths pay.
    fn payments(world: &World, payments: &mut Vec<Payment>) {
        let deaths = world.resource::<Deaths>();
        let Some(match_entity) = world.match_entity().filter(|_| !deaths.is_empty()) else {
            return;
        };
        let book = world.resource::<BountyBook>();
        let bounty = book.bounty().expect("the pass installs with a bounty");
        let index = world.resource::<EntityIndex>();
        let Some(relations) = world.get::<Relations>(match_entity) else {
            return;
        };
        for death in deaths {
            if !bounty.victim.selects_tags(death.fallen.tags) {
                continue;
            }
            let (Some(victim), Some(killer), Some(victim_team)) = (
                index.get(death.fallen.id),
                death.killer.and_then(|killer| index.get(killer)),
                death.fallen.team,
            ) else {
                continue;
            };
            let unit_type = world.get::<UnitType>(victim);
            let Some(&TypeBounty { resource, pays }) =
                unit_type.and_then(|&unit_type| bounty.types.get(unit_type))
            else {
                continue;
            };
            let (Some(owner), Some(&team), Some(tags)) = (
                world.get::<Owner>(killer),
                world.get::<Team>(killer),
                world.get::<UnitTags>(killer),
            ) else {
                continue;
            };
            if !bounty
                .killer
                .selects(relations.between(victim_team, team), tags.tags)
            {
                continue;
            }
            let player_slot = owner.player_slot();
            let amount = match pays {
                Pays::Amount(amount) => Some(i64::from(amount)),
                Pays::Cost(cost) => {
                    let player = world.player_entity(player_slot);
                    let share = player.and_then(|player| world.get::<BountyShare>(player));
                    let share = share.map_or(Num::ZERO, |share| share.0);
                    share.checked_mul_int(i64::from(cost)).map(Num::ceil)
                }
            };
            if let Some(amount) = amount.filter(|&amount| amount > 0) {
                payments.push(Payment {
                    player_slot,
                    resource,
                    amount,
                });
            }
        }
    }
}

/// A death's bounty: the player it pays, the resource and the amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Payment {
    player_slot: PlayerSlot,
    resource: ResourceId,
    amount: i64,
}
