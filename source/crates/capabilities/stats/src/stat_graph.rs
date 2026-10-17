use campfire_declare::{BookCx, BookError, BookPlace, Fact, Section};
use campfire_script::{NameKind, UnknownName};
use campfire_units::StatId;
use campfire_values::{NameId, PackageIndex, Param, Stat};

use crate::error::{EdgeError, StatGraphError};
use crate::mode_stats_data::ModeStatsData;
use crate::modifiers_data::ModifiersData;

/// Which stats a live stat change reads and which it changes, across a mode's modifiers: an edge
/// from each stat a scaling param reads to the stat the change that reads it changes. A fact of
/// the match, which the stats build of the modifiers' own params, and the modules whose data
/// applies a modifier extend with the params they give it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatGraph {
    stats: Vec<Stat>,
    /// Each edge's stats as places among `stats`, read then changed, sorted, each once.
    edges: Vec<StatEdge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct StatEdge {
    read: u16,
    changed: u16,
}

impl Fact for StatGraph {}

impl StatGraph {
    /// The graph of no edge among `stats`, the mode's stats in order.
    pub fn new(stats: impl IntoIterator<Item = Stat>) -> Self {
        Self {
            stats: stats.into_iter().collect(),
            edges: Vec::new(),
        }
    }

    /// The graph of the mode's stats, with the edges of each change that reads a param its
    /// modifier declares; an error for a stat the mode does not have, at the modifier.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let stats = cx.mode_section::<ModeStatsData>();
        let mut graph = Self::new(stats.into_iter().flat_map(|stats| stats.0.keys().cloned()));
        for (package, source) in cx.packages() {
            let modifiers = source.content.get::<ModifiersData>();
            for (id, modifier) in modifiers.into_iter().flat_map(|modifiers| &modifiers.0) {
                for (changed, change) in &modifier.stats {
                    let own = change
                        .value
                        .param()
                        .and_then(|name| modifier.params.get(name));
                    for read in own.into_iter().flat_map(Param::stats) {
                        graph.add(read, changed).map_err(|error| {
                            let at = BookPlace::Entry {
                                table: ModifiersData::KEY,
                                name: id.clone(),
                            };
                            BookError::unknown(package, at, error.name())
                        })?;
                    }
                }
            }
        }
        Ok(graph)
    }

    /// The stats in the order the stats refresh computes them, of the match's graph; an error
    /// for live stat changes that read each other in a loop, of the mode's stats.
    pub fn order_in(cx: &BookCx<'_>) -> Result<Vec<StatId>, BookError> {
        cx.fact::<Self>()?.order().map_err(|stats| {
            let at = BookPlace::Section(ModeStatsData::KEY);
            BookError::rule(PackageIndex::MODE, at, StatGraphError::Loop(stats))
        })
    }

    /// The rule of the live stat changes: none reads, through others, the stat it changes.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        Self::order_in(cx).map(drop)
    }

    /// Adds that a change of `changed` reads `read`; an error for the first of `changed` and
    /// `read` that is not a stat of the graph.
    pub fn add(&mut self, read: &Stat, changed: &Stat) -> Result<(), EdgeError> {
        let changed = self.place(changed).map_err(EdgeError::Changed)?;
        let read = self.place(read).map_err(EdgeError::Read)?;
        let edge = StatEdge { read, changed };
        if let Err(place) = self.edges.binary_search(&edge) {
            self.edges.insert(place, edge);
        }
        Ok(())
    }

    fn place(&self, stat: &Stat) -> Result<u16, UnknownName> {
        let at = self.stats.binary_search(stat).ok();
        let at = at.ok_or_else(|| UnknownName::new(NameKind::Stat, stat.name()))?;
        Ok(u16::try_from(at).expect("stats fit u16"))
    }

    /// The stats in an order where each comes after every stat a change of it reads, the lowest
    /// place first among those ready together; the stats of a loop when there is one.
    pub fn order(&self) -> Result<Vec<StatId>, Vec<Stat>> {
        let count = self.stats.len();
        let mut reads = vec![0_u32; count];
        for edge in &self.edges {
            reads[usize::from(edge.changed)] += 1;
        }
        let mut order = Vec::with_capacity(count);
        let mut done = vec![false; count];
        while order.len() < count {
            let Some(next) = (0..count).find(|&at| !done[at] && reads[at] == 0) else {
                let looped = (0..count).filter(|&at| !done[at]);
                return Err(looped.map(|at| self.stats[at].clone()).collect());
            };
            done[next] = true;
            order.push(StatId::at(next));
            let next = u16::try_from(next).expect("stats fit u16");
            for edge in self.edges.iter().filter(|edge| edge.read == next) {
                reads[usize::from(edge.changed)] -= 1;
            }
        }
        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_order_puts_each_stat_after_those_its_changes_read_and_a_loop_fails() {
        // Places: armor 0, power 1, vamp 2. Armor reads vamp, vamp reads power: power, vamp,
        // armor, where the places alone would give armor first.
        let [armor, power, vamp] =
            ["armor", "power", "vamp"].map(|name| Stat::named(name).unwrap());
        let mut graph = StatGraph::new([armor.clone(), power.clone(), vamp.clone()]);
        assert_eq!(graph.order(), Ok([0, 1, 2].map(StatId::new).to_vec()));
        graph.add(&vamp, &armor).unwrap();
        graph.add(&power, &vamp).unwrap();
        graph.add(&power, &vamp).unwrap();
        assert_eq!(graph.order(), Ok([1, 2, 0].map(StatId::new).to_vec()));
        // A stat the graph does not hold adds no edge: the changed one is named first.
        let mana = Stat::named("mana").unwrap();
        let unknown = UnknownName::new(NameKind::Stat, "mana");
        let refused = graph.clone();
        assert_eq!(
            graph.add(&mana, &armor),
            Err(EdgeError::Read(unknown.clone()))
        );
        assert_eq!(
            graph.add(&armor, &mana),
            Err(EdgeError::Changed(unknown.clone()))
        );
        assert_eq!(graph.add(&mana, &mana), Err(EdgeError::Changed(unknown)));
        assert_eq!(graph, refused);
        // Power reading armor closes a loop through all three; a stat reading itself is one.
        let mut looped = graph.clone();
        looped.add(&armor, &power).unwrap();
        assert_eq!(
            looped.order(),
            Err(vec![armor.clone(), power, vamp.clone()])
        );
        let mut own = StatGraph::new([armor.clone(), vamp]);
        own.add(&armor, &armor).unwrap();
        assert_eq!(own.order(), Err(vec![armor]));
    }
}
