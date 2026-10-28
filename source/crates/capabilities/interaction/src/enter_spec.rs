use campfire_actions::ActionBook;
use campfire_common::{PlayerSlot, Ticks};
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_math::Num;
use campfire_script::{Filter, Filters, UnknownName};
use campfire_stats::ModifierBook;
use campfire_units::UnitTypes;
use campfire_values::{
    ActionId, ModifierId, Named, Relation, Share, TagSet, TimeTooLarge, TypeScope, UnitType,
};

use crate::enter_data::{EnterData, FireData, RiderCount};

/// A container type's `enter` as a match reads it, package data and not state
/// (04-capabilities/interaction.md, A container says who rides): its entry, how many it holds and
/// how it counts them, the riders it takes, the ticks between two exits, the share of each rider's
/// life most its death deals it, the share of its own life at or below which it holds none, whether
/// it takes its first rider's player, the riders it spawns with, where its riders fire from,
/// whether they see from inside, and the modifier each rider holds inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnterSpec {
    pub action: ActionId,
    pub capacity: u32,
    pub counts: RiderCount,
    pub riders: Filters,
    pub exit: Ticks,
    pub damage: Share,
    pub full: Share,
    pub capture: bool,
    pub payload: Box<[RiderPayload]>,
    pub fire: FireSpec,
    pub sight: bool,
    pub modifier: Option<ModifierId>,
    pub open: Option<Filter>,
    pub assault: Option<Share>,
}

/// Where a container's riders fire from, its names resolved (04-capabilities/combat.md, A container
/// may let its riders fire).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FireSpec {
    None,
    /// Its own place, its riders whose tags the filter selects.
    Center(Filter),
    /// Its fire points, `[x, z]` of its frame, one at least: each rider fires from the free one
    /// nearest its target.
    Points(Box<[[Num; 2]]>),
}

impl FireSpec {
    /// Its fire points, none but for a container of points.
    pub fn points(&self) -> &[[Num; 2]] {
        match self {
            Self::Points(points) => points,
            Self::None | Self::Center(_) => &[],
        }
    }

    /// Whether a rider of `tags` fires from the container.
    pub fn fires(&self, tags: TagSet) -> bool {
        match self {
            Self::None => false,
            Self::Center(filter) => filter.selects_tags(tags),
            Self::Points(_) => true,
        }
    }
}

/// Riders a container spawns with: `count` of `unit_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RiderPayload {
    pub unit_type: UnitType,
    pub count: u32,
}

impl EnterSpec {
    /// Whether its riders' filters take a unit of `tags` whose team the container's regards
    /// with `relation`.
    pub fn admits(&self, relation: Relation, tags: TagSet) -> bool {
        self.riders.selects(relation, tags)
    }

    /// Whether a container of it of `tags` takes riders: one of no `open`, or of tags it selects
    /// (04-capabilities/interaction.md, A bunker is an entry a tag opens).
    pub fn opens(&self, tags: TagSet) -> bool {
        self.open
            .as_ref()
            .is_none_or(|open| open.selects_tags(tags))
    }

    /// Whether a rider of `rider`'s player may enter a container of it that `held_by`'s player
    /// holds: one of its own player's, or a free building no player holds
    /// (04-capabilities/interaction.md, A rider may enter its own player's container, or a free
    /// one).
    pub fn sides(&self, held_by: Option<PlayerSlot>, rider: Option<PlayerSlot>) -> bool {
        held_by.map_or(self.capture, |held_by| Some(held_by) == rider)
    }

    /// The room a rider of `size` takes of it.
    pub const fn room_of(&self, size: u32) -> u32 {
        match self.counts {
            RiderCount::Size => size,
            RiderCount::One => 1,
        }
    }
}

impl PerId<UnitType> for EnterSpec {
    const SCRIPTS_ONLY: bool = false;
}

impl EnterSpec {
    /// The `enter` of each container type that stands, its entry among its package's actions,
    /// its riders and its fire's filter among the match's tags, its payload among its package's
    /// types and its modifier among its package's; an error for a name the package or the match
    /// does not have, or an exit too long to count in ticks.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let actions = cx.book::<ActionBook>()?;
        let modifiers = cx.book_if::<ModifierBook>()?;
        let tags = cx.fact::<UnitTypes>()?.tags();
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<EnterData>() else {
                continue;
            };
            let at = || BookPlace::UnitType(unit.place());
            let unknown = |name| BookError::unknown(unit.package, at(), name);
            let action = actions.names().named((unit.package, data.action.as_str()));
            let action =
                action.ok_or_else(|| unknown(UnknownName::new(ActionId::NAMES, &data.action)))?;
            let riders = Filters::resolve(&data.riders, tags).map_err(unknown)?;
            let exit = cx.rate().ticks(u64::from(data.exit_ms));
            let exit = exit.ok_or_else(|| BookError::rule(unit.package, at(), TimeTooLarge))?;
            let scope = TypeScope::of_package(unit.package);
            let payload = data.payload.iter().map(|payload| {
                let unit_type = cx.unit_type(scope, payload.unit.as_str());
                let unit_type = unit_type
                    .ok_or_else(|| unknown(UnknownName::new(UnitType::NAMES, &payload.unit)))?;
                Ok(RiderPayload {
                    unit_type,
                    count: payload.count,
                })
            });
            let fire = match &data.fire {
                FireData::None => FireSpec::None,
                FireData::Center(filter) => {
                    FireSpec::Center(Filter::resolve(filter.filter(), tags).map_err(unknown)?)
                }
                FireData::Points(points) => FireSpec::Points(points.iter().copied().collect()),
            };
            let open = data
                .open
                .as_ref()
                .map(|open| Filter::resolve(open.filter(), tags));
            let modifier = data.modifier.as_ref().map(|name| {
                let modifier =
                    modifiers.and_then(|book| book.names().named((unit.package, name.as_str())));
                modifier.ok_or_else(|| unknown(UnknownName::new(ModifierId::NAMES, name)))
            });
            let spec = Self {
                action,
                capacity: data.capacity,
                counts: data.counts,
                riders,
                exit,
                damage: data.damage,
                full: data.full,
                capture: data.capture,
                payload: payload.collect::<Result<_, BookError>>()?,
                fire,
                sight: data.sight,
                modifier: modifier.transpose()?,
                open: open.transpose().map_err(unknown)?,
                assault: data.assault,
            };
            book.set(unit.unit_type, spec);
        }
        Ok(book.build())
    }
}
