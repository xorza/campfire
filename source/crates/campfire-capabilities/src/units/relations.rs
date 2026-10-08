use std::array;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_sim::SimResource;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::units::script_view::View;
use crate::units::team::Team;
use crate::units::team_set::TeamSet;
use crate::values::attitude::Attitude;

/// How the match's teams regard each other, and which friendly pairs share vision: state, which
/// `ctx.set_relation` changes. A pair it does not hold is hostile, and would share vision were it
/// friendly; a team is friendly to itself.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Relations {
    /// The pairs that differ from the default, the lower team first, sorted by their teams.
    pairs: Vec<RelationPair>,
}

/// A pair of teams, `a` below `b`, as they regard each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct RelationPair {
    a: Team,
    b: Team,
    attitude: Attitude,
    vision: bool,
}

impl Relations {
    /// How `of` regards `other`, and so `other` regards `of`.
    pub(crate) fn between(&self, of: Team, other: Team) -> Attitude {
        if of == other {
            return Attitude::Friendly;
        }
        self.find(of, other)
            .ok()
            .map_or(Attitude::Hostile, |at| self.pairs[at].attitude)
    }

    /// Sets how the two teams, which differ, regard each other, and whether, friendly, they
    /// share vision.
    pub(crate) fn set(&mut self, of: Team, other: Team, attitude: Attitude, vision: bool) {
        debug_assert_ne!(of, other, "a team's relation to itself is friendly");
        let (a, b) = (of.min(other), of.max(other));
        let default = attitude == Attitude::Hostile && vision;
        match (self.find(a, b), default) {
            (Ok(at), true) => drop(self.pairs.remove(at)),
            (Ok(at), false) => {
                self.pairs[at].attitude = attitude;
                self.pairs[at].vision = vision;
            }
            (Err(at), false) => self.pairs.insert(
                at,
                RelationPair {
                    a,
                    b,
                    attitude,
                    vision,
                },
            ),
            (Err(_), true) => {}
        }
    }

    /// Sets how the two teams regard each other, their vision as it was.
    pub(crate) fn set_attitude(&mut self, of: Team, other: Team, attitude: Attitude) {
        let vision = self
            .find(of, other)
            .ok()
            .is_none_or(|at| self.pairs[at].vision);
        self.set(of, other, attitude, vision);
    }

    /// Each pair of teams that share vision, the lower team first.
    pub(crate) fn vision_pairs(&self) -> impl Iterator<Item = (Team, Team)> + '_ {
        self.pairs
            .iter()
            .filter(|pair| pair.attitude == Attitude::Friendly && pair.vision)
            .map(|pair| (pair.a, pair.b))
    }

    /// The teams that see as one with `team`: those it shares vision with, those they share it
    /// with, and so on, `team` among them.
    pub(crate) fn vision_group(&self, team: Team) -> TeamSet {
        let lowest = self.vision_lowest();
        let own = lowest[usize::from(team.index())];
        (0..=u8::MAX)
            .filter(|&at| lowest[usize::from(at)] == own)
            .fold(TeamSet::NONE, |group, at| group.with(Team::new(at)))
    }

    /// The lowest team of each team's vision group, by team index, from one pass over the pairs
    /// that share vision.
    pub(crate) fn vision_lowest(&self) -> [u8; Team::LIMIT] {
        let mut root: [u8; Team::LIMIT] =
            array::from_fn(|at| u8::try_from(at).expect("a team index fits u8"));
        let find = |root: &mut [u8; Team::LIMIT], mut at: u8| {
            while root[usize::from(at)] != at {
                let up = root[usize::from(at)];
                root[usize::from(at)] = root[usize::from(up)];
                at = up;
            }
            at
        };
        for (a, b) in self.vision_pairs() {
            let (a, b) = (find(&mut root, a.index()), find(&mut root, b.index()));
            root[usize::from(a.max(b))] = a.min(b);
        }
        // Every link points to a lower team, so in order each team's link is final already.
        for at in 0..Team::LIMIT {
            root[at] = root[usize::from(root[at])];
        }
        root
    }

    fn find(&self, of: Team, other: Team) -> Result<usize, usize> {
        let key = (of.min(other), of.max(other));
        self.pairs
            .binary_search_by_key(&key, |pair| (pair.a, pair.b))
    }
}

impl SimResource for Relations {
    const NAME: &'static str = "units.relations";

    // A relation of a team the mode lacks would join it to a vision group of the mode's teams.
    fn check(&self, world: &World) -> bool {
        world.get_non_send::<View>().is_none_or(|view| {
            self.pairs
                .iter()
                .all(|pair| view.has_team(pair.a) && view.has_team(pair.b))
        })
    }
}

/// A snapshot is untrusted, so pairs out of order, of a team with itself, or that are the
/// default fail to decode.
impl<'de> Deserialize<'de> for Relations {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Relations, D::Error> {
        let pairs = Vec::<RelationPair>::deserialize(deserializer)?;
        let ordered = pairs
            .windows(2)
            .all(|two| (two[0].a, two[0].b) < (two[1].a, two[1].b));
        let held = |pair: &RelationPair| {
            pair.a < pair.b && !(pair.attitude == Attitude::Hostile && pair.vision)
        };
        if !ordered || !pairs.iter().all(held) {
            return Err(D::Error::custom(
                "relations out of order or held needlessly",
            ));
        }
        Ok(Relations { pairs })
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use bevy_ecs::world::World;

    use crate::units::relations::Relations;
    use crate::units::team::Team;
    use crate::values::attitude::Attitude;

    /// Sets how the two teams of the match in `world` regard each other, as `ctx.set_relation`
    /// does, their vision as it was.
    pub fn set_relation(world: &mut World, of: Team, other: Team, attitude: Attitude) {
        world
            .resource_mut::<Relations>()
            .set_attitude(of, other, attitude);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pair_is_hostile_until_set_and_only_friends_share_vision() {
        let [a, b, c] = [0, 1, 200].map(Team::new);
        let mut relations = Relations::default();
        assert_eq!(relations.between(a, a), Attitude::Friendly);
        assert_eq!(relations.between(a, b), Attitude::Hostile);
        assert_eq!(relations.vision_pairs().count(), 0);
        // Set either way round, a pair reads the same both ways.
        relations.set(c, a, Attitude::Friendly, true);
        relations.set(b, a, Attitude::Neutral, true);
        assert_eq!(relations.between(a, c), Attitude::Friendly);
        assert_eq!(relations.between(b, a), Attitude::Neutral);
        assert_eq!(relations.vision_pairs().collect::<Vec<_>>(), [(a, c)]);
        // Vision off holds through a change of attitude; back to hostile with vision on, the pair
        // is the default and no longer held.
        relations.set(a, c, Attitude::Friendly, false);
        relations.set_attitude(c, a, Attitude::Neutral);
        relations.set_attitude(c, a, Attitude::Friendly);
        assert_eq!(relations.vision_pairs().count(), 0);
        relations.set(a, b, Attitude::Hostile, true);
        assert_eq!(relations.pairs.len(), 1);
        // A snapshot that holds a default pair fails to decode.
        let bytes = postcard::to_allocvec(&vec![(a, b, Attitude::Hostile, true)]).unwrap();
        assert!(postcard::from_bytes::<Relations>(&bytes).is_err());
        let bytes = postcard::to_allocvec(&relations).unwrap();
        assert_eq!(
            postcard::from_bytes::<Relations>(&bytes).ok(),
            Some(relations)
        );
    }
}
