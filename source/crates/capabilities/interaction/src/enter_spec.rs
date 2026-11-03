use campfire_actions::ActionBook;
use campfire_common::{PlayerSlot, Ticks};
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_math::Num;
use campfire_names::{Filter, Filters, UnknownName};
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
    pub fire_when: Option<Filter>,
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
    /// The builder of the spec of entry `action` and `capacity`.
    pub fn builder(action: ActionId, capacity: u32) -> EnterSpecBuilder {
        EnterSpecBuilder(Self {
            action,
            capacity,
            counts: RiderCount::Size,
            riders: Filters::default(),
            exit: Ticks::ZERO,
            damage: Share::NONE,
            full: Share::NONE,
            capture: false,
            payload: Box::new([]),
            fire: FireSpec::None,
            fire_when: None,
            sight: false,
            modifier: None,
            open: None,
            assault: None,
        })
    }

    /// Whether its riders' filters take a unit of `tags` whose team the container's regards
    /// with `relation`.
    pub fn admits(&self, relation: Relation, tags: TagSet) -> bool {
        self.riders.selects(relation, tags)
    }

    /// Whether a rider of `rider` fires from a container of it of `tags`: one its fire takes,
    /// while the container's tags pass its `fire_when` (riders-and-upgrades.md, A rider's fire
    /// follows its container's tags).
    pub fn rider_fires(&self, rider: TagSet, tags: TagSet) -> bool {
        self.fire.fires(rider)
            && self
                .fire_when
                .as_ref()
                .is_none_or(|when| when.selects_tags(tags))
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
}

/// An `EnterSpec` of its entry and capacity, each other field as a container of no `enter` field
/// set has it: riders counted by size, none selected, no exit time, no damage or full share, no
/// capture, payload, fire, sight, modifier, open tags or assault.
#[derive(Debug, Clone)]
pub struct EnterSpecBuilder(EnterSpec);

impl EnterSpecBuilder {
    #[must_use]
    pub const fn counts(mut self, counts: RiderCount) -> Self {
        self.0.counts = counts;
        self
    }

    #[must_use]
    pub fn riders(mut self, riders: Filters) -> Self {
        self.0.riders = riders;
        self
    }

    #[must_use]
    pub const fn exit(mut self, exit: Ticks) -> Self {
        self.0.exit = exit;
        self
    }

    #[must_use]
    pub const fn damage(mut self, damage: Share) -> Self {
        self.0.damage = damage;
        self
    }

    #[must_use]
    pub const fn full(mut self, full: Share) -> Self {
        self.0.full = full;
        self
    }

    #[must_use]
    pub const fn capture(mut self, capture: bool) -> Self {
        self.0.capture = capture;
        self
    }

    #[must_use]
    pub fn payload(mut self, payload: Box<[RiderPayload]>) -> Self {
        self.0.payload = payload;
        self
    }

    #[must_use]
    pub fn fire(mut self, fire: FireSpec) -> Self {
        self.0.fire = fire;
        self
    }

    #[must_use]
    pub const fn fire_when(mut self, fire_when: Option<Filter>) -> Self {
        self.0.fire_when = fire_when;
        self
    }

    #[must_use]
    pub const fn sight(mut self, sight: bool) -> Self {
        self.0.sight = sight;
        self
    }

    #[must_use]
    pub const fn modifier(mut self, modifier: Option<ModifierId>) -> Self {
        self.0.modifier = modifier;
        self
    }

    #[must_use]
    pub const fn open(mut self, open: Option<Filter>) -> Self {
        self.0.open = open;
        self
    }

    #[must_use]
    pub const fn assault(mut self, assault: Option<Share>) -> Self {
        self.0.assault = assault;
        self
    }

    /// The spec; a payload of more riders than its capacity is a caller's bug.
    pub fn build(self) -> EnterSpec {
        let spec = self.0;
        debug_assert!(
            spec.payload
                .iter()
                .map(|each| u64::from(each.count))
                .sum::<u64>()
                <= u64::from(spec.capacity),
            "a payload fits its container"
        );
        spec
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
                let unit_type = cx.type_named(scope, payload.unit.as_str());
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
            let fire_when = data
                .fire_when
                .as_ref()
                .map(|when| Filter::resolve(when.filter(), tags));
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
                fire_when: fire_when.transpose().map_err(unknown)?,
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
