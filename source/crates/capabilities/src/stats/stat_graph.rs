use crate::stats::stat::Stat;
use crate::stats::stat_id::StatId;

/// Which stats a live stat change reads and which it changes, across a mode's modifiers: an edge
/// from each stat a scaling param reads to the stat the change that reads it changes.
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

impl StatGraph {
    /// The graph of no edge among `stats`, the mode's stats in order.
    pub fn new(stats: impl IntoIterator<Item = Stat>) -> StatGraph {
        StatGraph {
            stats: stats.into_iter().collect(),
            edges: Vec::new(),
        }
    }

    /// Adds that a change of `changed` reads `read`; both are stats of the graph, which the load
    /// checked.
    pub fn add(&mut self, read: &Stat, changed: &Stat) {
        let at = |stat| {
            let at = self.stats.binary_search(stat).expect("a stat of the mode");
            u16::try_from(at).expect("stats fit u16")
        };
        let edge = StatEdge {
            read: at(read),
            changed: at(changed),
        };
        if let Err(place) = self.edges.binary_search(&edge) {
            self.edges.insert(place, edge);
        }
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
            order.push(StatId::new(next));
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
        graph.add(&vamp, &armor);
        graph.add(&power, &vamp);
        graph.add(&power, &vamp);
        assert_eq!(graph.order(), Ok([1, 2, 0].map(StatId::new).to_vec()));
        // Power reading armor closes a loop through all three; a stat reading itself is one.
        let mut looped = graph.clone();
        looped.add(&armor, &power);
        assert_eq!(
            looped.order(),
            Err(vec![armor.clone(), power, vamp.clone()])
        );
        let mut own = StatGraph::new([armor.clone(), vamp]);
        own.add(&armor, &armor);
        assert_eq!(own.order(), Err(vec![armor]));
    }
}
