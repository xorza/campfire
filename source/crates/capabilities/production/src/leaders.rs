use bevy_ecs::query::{Changed, With};
use bevy_ecs::system::{Local, ParamSet, Query, Res};
use campfire_math::{Num, Vec3};
use campfire_sim::{EntityIndex, Position, StableId};
use campfire_units::{Leader, LivingSpawns, SpawnOf};

/// The leaders of spawns, which stand at their spawns' centre.
#[derive(Debug)]
pub(crate) struct Leaders;

impl Leaders {
    /// Puts each leader at the centre of its living spawns, written in place, as OpenRA's port of
    /// the mob sets its nexus (spawn-leaders.md, A unit may lead its spawns): only the leaders of
    /// the spawns whose place changed, and those whose living spawns changed, in a buffer it
    /// keeps; a leader of none stays where it stood.
    pub(crate) fn centre(
        mut units: ParamSet<
            '_,
            '_,
            (
                Query<'_, '_, &SpawnOf, Changed<Position>>,
                Query<'_, '_, &Position>,
                Query<'_, '_, &mut Position, With<Leader>>,
            ),
        >,
        changed: Query<'_, '_, &StableId, (With<Leader>, Changed<LivingSpawns>)>,
        leaders: Query<'_, '_, &LivingSpawns, With<Leader>>,
        index: Res<'_, EntityIndex>,
        mut due: Local<'_, Vec<StableId>>,
    ) {
        due.clear();
        due.extend(units.p0().iter().map(|spawn_of| spawn_of.spawner()));
        due.extend(changed.iter().copied());
        due.sort_unstable();
        due.dedup();
        for &leader in &*due {
            let Some(entity) = index.get(leader) else {
                continue;
            };
            let Ok(living) = leaders.get(entity) else {
                continue;
            };
            let places = units.p1();
            let mut sum = Vec3::ZERO;
            let mut count = 0;
            for &spawn in living {
                if let Some(at) = index.get(spawn).and_then(|spawn| places.get(spawn).ok()) {
                    sum += at.get();
                    count += 1;
                }
            }
            if count == 0 {
                continue;
            }
            let centre = sum / Num::int(count);
            if let Ok(mut place) = units.p2().get_mut(entity)
                && place.get() != centre
            {
                *place = Position::from(centre);
            }
        }
    }
}
