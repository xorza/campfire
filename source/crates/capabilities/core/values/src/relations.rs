use std::array;

#[cfg(feature = "ecs")]
use bevy_ecs::component::Component;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::names::NameId;
use crate::relation::Relation;
use crate::team::Team;
use crate::team_set::TeamSet;

/// How the match's teams regard each other, and which friendly pairs share vision: state on the
/// match's entity, which `ctx.set_relation` changes. A pair it does not hold is hostile, and
/// would share vision were it friendly; a team is friendly to itself.
#[cfg_attr(feature = "ecs", derive(Component))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
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
    relation: Relation,
    vision: VisionShare,
}

/// Whether two friendly teams see as one, sharing what each sees, or each alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisionShare {
    Shared,
    Apart,
}

impl Relations {
    /// How `of` regards `other`, and so `other` regards `of`.
    pub fn between(&self, of: Team, other: Team) -> Relation {
        if of == other {
            return Relation::Friendly;
        }
        self.find(of, other)
            .ok()
            .map_or(Relation::Hostile, |at| self.pairs[at].relation)
    }

    /// Sets how the two teams, which differ, regard each other, and whether, friendly, they
    /// share `vision`.
    pub fn set(&mut self, of: Team, other: Team, relation: Relation, vision: VisionShare) {
        debug_assert_ne!(of, other, "a team's relation to itself is friendly");
        let (a, b) = (of.min(other), of.max(other));
        let default = relation == Relation::Hostile && vision == VisionShare::Shared;
        match (self.find(a, b), default) {
            (Ok(at), true) => drop(self.pairs.remove(at)),
            (Ok(at), false) => {
                self.pairs[at].relation = relation;
                self.pairs[at].vision = vision;
            }
            (Err(at), false) => self.pairs.insert(
                at,
                RelationPair {
                    a,
                    b,
                    relation,
                    vision,
                },
            ),
            (Err(_), true) => {}
        }
    }

    /// Sets how the two teams regard each other, their vision as it was.
    pub fn set_keeping_vision(&mut self, of: Team, other: Team, relation: Relation) {
        let vision = self
            .find(of, other)
            .ok()
            .map_or(VisionShare::Shared, |at| self.pairs[at].vision);
        self.set(of, other, relation, vision);
    }

    /// Each team a pair it holds names, as a restore checks them against the mode's.
    pub fn teams(&self) -> impl Iterator<Item = Team> + '_ {
        self.pairs.iter().flat_map(|pair| [pair.a, pair.b])
    }

    /// Each pair of teams that share vision, the lower team first.
    pub(crate) fn vision_pairs(&self) -> impl Iterator<Item = (Team, Team)> + '_ {
        self.pairs
            .iter()
            .filter(|pair| {
                pair.relation == Relation::Friendly && pair.vision == VisionShare::Shared
            })
            .map(|pair| (pair.a, pair.b))
    }

    /// The teams that see as one with `team`: those it shares vision with, those they share it
    /// with, and so on, `team` among them.
    pub fn vision_group(&self, team: Team) -> TeamSet {
        let lowest = self.vision_lowest();
        let own = lowest[team.index()];
        (0..=u8::MAX)
            .filter(|&at| lowest[usize::from(at)] == own)
            .fold(TeamSet::NONE, |group, at| group.with(Team::new(at)))
    }

    /// The lowest team of each team's vision group, by team index, from one pass over the pairs
    /// that share vision.
    pub fn vision_lowest(&self) -> [u8; Team::LIMIT] {
        let mut root: [u8; Team::LIMIT] =
            array::from_fn(|at| u8::try_from(at).expect("a team index fits u8"));
        let find = |root: &mut [u8; Team::LIMIT], mut at: usize| {
            while usize::from(root[at]) != at {
                let up = usize::from(root[at]);
                root[at] = root[up];
                at = up;
            }
            at
        };
        for (a, b) in self.vision_pairs() {
            let (a, b) = (find(&mut root, a.index()), find(&mut root, b.index()));
            root[a.max(b)] = u8::try_from(a.min(b)).expect("a team index fits u8");
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

/// A snapshot is untrusted, so pairs out of order, of a team with itself, or that are the
/// default fail to decode.
impl<'de> Deserialize<'de> for Relations {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let pairs = Vec::<RelationPair>::deserialize(deserializer)?;
        let ordered = pairs
            .windows(2)
            .all(|two| (two[0].a, two[0].b) < (two[1].a, two[1].b));
        let held = |pair: &RelationPair| {
            pair.a < pair.b
                && !(pair.relation == Relation::Hostile && pair.vision == VisionShare::Shared)
        };
        if !ordered || !pairs.iter().all(held) {
            return Err(D::Error::custom(
                "relations out of order or held needlessly",
            ));
        }
        Ok(Self { pairs })
    }
}

#[cfg(feature = "ecs")]
mod ecs {
    use bevy_ecs::entity::Entity;
    use bevy_ecs::world::World;
    use campfire_sim::{OnAny, SimComponent};
    use campfire_states::{Replication, SentOnChange};
    use campfire_visibility::DataKind;

    use crate::relations::Relations;

    impl SimComponent for Relations {
        const NAME: &'static str = "units.relations";
        type Host = OnAny;

        // `units` declares its host, the match's entity, and checks its teams against the mode's.
        fn check(&self, _: &World, _: Entity) -> bool {
            true
        }
    }

    impl Replication for Relations {
        const KIND: DataKind = DataKind::Match;
        type Sending = SentOnChange;
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::{Binary, BinaryError};

    use super::*;

    #[test]
    fn a_pair_is_hostile_until_set_and_only_friends_share_vision() {
        let [a, b, c] = [0, 1, 200].map(Team::new);
        let mut relations = Relations::default();
        assert_eq!(relations.between(a, a), Relation::Friendly);
        assert_eq!(relations.between(a, b), Relation::Hostile);
        assert_eq!(relations.vision_pairs().count(), 0);
        // Set either way round, a pair reads the same both ways.
        relations.set(c, a, Relation::Friendly, VisionShare::Shared);
        relations.set(b, a, Relation::Neutral, VisionShare::Shared);
        assert_eq!(relations.between(a, c), Relation::Friendly);
        assert_eq!(relations.between(b, a), Relation::Neutral);
        assert_eq!(relations.vision_pairs().collect::<Vec<_>>(), [(a, c)]);
        // Vision off holds through a change of relation; back to hostile with vision on, the pair
        // is the default and no longer held.
        relations.set(a, c, Relation::Friendly, VisionShare::Apart);
        relations.set_keeping_vision(c, a, Relation::Neutral);
        relations.set_keeping_vision(c, a, Relation::Friendly);
        assert_eq!(relations.vision_pairs().count(), 0);
        relations.set(a, b, Relation::Hostile, VisionShare::Shared);
        assert_eq!(relations.pairs.len(), 1);
        // A snapshot that holds a default pair fails to decode.
        let bytes = Binary::encode(&vec![(a, b, Relation::Hostile, VisionShare::Shared)]);
        assert_eq!(
            Binary::decode::<Relations>(&bytes).err(),
            Some(BinaryError::REFUSED)
        );
        let bytes = Binary::encode(&relations);
        assert_eq!(Binary::decode::<Relations>(&bytes).ok(), Some(relations));
    }
}
