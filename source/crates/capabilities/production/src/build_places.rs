use bevy_ecs::query::{Allow, With, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_actions::ActionSlots;
use campfire_geometry::GridSearch;
use campfire_math::{Num, Vec3};
use campfire_sim::{EntityIndex, Position, StableId, Unpredicted};
use campfire_units::{Dead, GroundPoint, Owner};
use campfire_values::{Team, UnitType};
use campfire_vision::TeamSight;

use crate::build_book::BuildBook;
use crate::placement::Placement;
use crate::site::Site;

/// What a search for a build's place reads: the builds, of a match with production, the builders,
/// and the placement a build order tests, by `S` (04-capabilities/control.md, A bot finds a
/// building's place as the build tests it).
#[derive(SystemParam, Debug)]
pub struct BuildPlaces<'w, 's, S: TeamSight> {
    index: Res<'w, EntityIndex>,
    builds: Option<Res<'w, BuildBook>>,
    builders: Query<
        'w,
        's,
        (
            &'static Position,
            &'static Team,
            &'static ActionSlots,
            Option<&'static Owner>,
        ),
        (Without<Dead>, Allow<Unpredicted>),
    >,
    sites: Query<
        'w,
        's,
        (&'static UnitType, Option<&'static Owner>),
        (With<Site>, Without<Dead>, Allow<Unpredicted>),
    >,
    placement: Placement<'w, 's, S>,
}

/// A search for the place of a build: the builder, the place among its slots of the build's
/// action, the point the search starts from, the gap of its grid and how many gaps it reaches,
/// and the angle the build's box turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceSearch {
    pub builder: StableId,
    pub slot: u8,
    pub near: GroundPoint,
    pub gap: Num,
    pub steps: u32,
    pub angle: Num,
}

impl<S: TeamSight> BuildPlaces<'_, '_, S> {
    /// The nearest point of the search's grid, at the builder's height, as its order places the
    /// build, where the build its slot holds passes its placement for the builder's team, as its
    /// order tests it; none for a builder that is gone, a slot that holds no build, a match with
    /// no builds, or no point that passes.
    pub fn nearest(&self, search: PlaceSearch) -> Option<GroundPoint> {
        let PlaceSearch {
            builder,
            slot,
            near,
            gap,
            steps,
            angle,
        } = search;
        let (from, &team, slots, _) = self.builders.get(self.index.get(builder)?).ok()?;
        let spec = self.builds.as_deref()?.of(slots.slot(slot)?.action?)?;
        let body = spec.form.at(angle);
        let grid = GridSearch {
            position: Vec3::new(near.x, from.get().y, near.z),
            gap,
            steps,
        };
        let found = grid.nearest(|at| self.placement.passes(team, at, body, spec));
        found.map(GroundPoint::under)
    }

    /// Whether the build `builder` holds in `slot` continues the living site `site`: one of its
    /// building and its player, as a build order at a site takes it (04-capabilities/control.md, A
    /// bot's search runs its order's own test).
    pub fn takes_site(&self, builder: StableId, slot: u8, site: StableId) -> bool {
        let found = |id| self.index.get(id);
        let Some((_, _, slots, owner)) = found(builder).and_then(|at| self.builders.get(at).ok())
        else {
            return false;
        };
        let spec = slots
            .slot(slot)
            .and_then(|held| self.builds.as_deref()?.of(held.action?));
        let site = found(site).and_then(|at| self.sites.get(at).ok());
        spec.zip(site)
            .is_some_and(|(spec, (&site_type, site_owner))| {
                spec.continues(site_type, site_owner, owner)
            })
    }
}
