use campfire_common::Tick;
use campfire_geometry::{Bounds, Metric, Radius};
use campfire_math::SplitMix64;
use campfire_sim::IdAllocator;
use campfire_values::{Relation, Relations, UnitTags, UnitType, VisionShare};

use super::*;
use crate::scripts::view::tag_names::TagNames;
use crate::scripts::view::view_rows::ReadOf;

/// The tags of these tests, and the filters they parse of them.
#[derive(Debug)]
struct Tags {
    names: TagNames,
    sets: [TagSet; 4],
}

impl Tags {
    /// `hero`, `creep` and `tower`, and the sets of none, a hero, a creep, and a creep that is a
    /// tower too.
    fn new() -> Self {
        let mut names = TagNames::default();
        let [hero, creep, tower] =
            ["hero", "creep", "tower"].map(|name| names.names_mut().add(name));
        let none = TagSet::NONE;
        Self {
            names,
            sets: [
                none,
                none.with(hero),
                none.with(creep),
                none.with(creep).with(tower),
            ],
        }
    }

    fn filter(&self, text: &str) -> Filter {
        Filter::parse(text, &self.names).unwrap()
    }
}

/// A target row of `team` and `tags` at (`x`, `z`), of a disc of `radius` meters, by `id`.
fn row(id: StableId, team: u8, tags: TagSet, [x, z]: [i64; 2], radius: i64) -> UnitRow {
    UnitRow {
        id,
        position: Vec3::new(Num::int(x) / 10, Num::ZERO, Num::int(z) / 10),
        team: Team::new(team),
        shape: Shape::Circle(Radius::new(Num::int(radius) / 10).unwrap()),
        spawn: None,
        unit_type: UnitType::new(0),
        owner: None,
        alive: true,
        targetable: true,
        tags: UnitTags {
            tags,
            ..UnitTags::default()
        },
    }
}

/// View rows of `units`, teams 0 and 1 allied and team 2 the enemy of both.
fn rows(units: Vec<UnitRow>) -> ViewRows {
    let mut relations = Relations::default();
    relations.set(
        Team::new(0),
        Team::new(1),
        Relation::Friendly,
        VisionShare::Shared,
    );
    let mut rows = ViewRows::default();
    rows.begin(ReadOf {
        now: Tick::ZERO,
        relations: &relations,
        metric: Metric::Planar,
        bounds: Bounds::WORLD,
    });
    *rows.units_mut() = units;
    rows
}

/// The rows `query` reaches and `seen` lets, by a test of every row, in row order.
fn walked(rows: &ViewRows, query: TargetQuery<'_>, seen: impl Fn(usize) -> bool) -> Vec<usize> {
    let from = Placed::new(query.from, query.shape);
    (0..rows.units().len())
        .filter(|&at| {
            let row = &rows.units()[at];
            let relation = rows.relations().between(query.team, row.team);
            row.targetable
                && query.filter.selects(relation, row.tags.tags)
                && rows
                    .metric()
                    .reaches(from, query.radius, Placed::new(row.position, row.shape))
                && seen(at)
        })
        .collect()
}

#[test]
fn a_query_finds_what_a_walk_of_every_row_finds_and_charges_the_classes_it_selects() {
    // 60 targets of three teams and four tag sets within 20 m, some not targetable, over six
    // reads: after the first, each unit may be drawn again, move a tenth of a meter or leave,
    // and the fourth repeats the third, so every class keeps its grid; each read asks a sweep of
    // filters, reaches and teams, whose `find` and `visit` give the rows a test of every row
    // gives.
    let tags = Tags::new();
    let filters = [
        "all",
        "enemies",
        "allies:hero",
        "enemies:creep:!tower",
        "all:tower",
    ]
    .map(|text| tags.filter(text));
    let mut random = SplitMix64::new(7);
    let drawn = |random: &mut SplitMix64, id| {
        let team = u8::try_from(random.below(3)).unwrap();
        let set = tags.sets[usize::try_from(random.below(4)).unwrap()];
        let at = [0, 1].map(|_| i64::try_from(random.below(400)).unwrap() - 200);
        let mut unit = row(id, team, set, at, i64::try_from(random.below(15)).unwrap());
        unit.targetable = random.below(8) != 0;
        unit
    };
    let mut ids = IdAllocator::default();
    let mut units: Vec<UnitRow> = (0..60).map(|_| drawn(&mut random, ids.allocate())).collect();
    let mut index = TargetIndex::default();
    let charge = CallCharge::default();
    for read in 0..6 {
        if read > 0 && read != 3 {
            for unit in &mut units {
                match random.below(6) {
                    0 => *unit = drawn(&mut random, unit.id),
                    1 => unit.position.x += Num::ONE / 10,
                    _ => {}
                }
            }
            units.retain(|_| random.below(20) != 0);
        }
        let rows = rows(units.clone());
        index.forget();
        for (filter, (team, radius)) in filters
            .iter()
            .flat_map(|filter| [(0, 30), (1, 80), (2, 0)].map(|query| (filter, query)))
        {
            let query = TargetQuery {
                team: Team::new(team),
                from: Vec3::new(Num::int(3), Num::ZERO, Num::int(-2)),
                shape: Shape::Circle(Radius::new(Num::HALF).unwrap()),
                radius: Num::int(radius) / 10,
                filter,
            };
            let seen = |at: usize| at % 7 != 3;
            let expected = walked(&rows, query, seen);
            let found = index.find(&rows, query, (|_: &ViewColumns, at| seen(at), &charge));
            assert_eq!(found, expected, "read {read}");
            let mut visited = Vec::new();
            index.visit(
                &rows,
                query,
                (|_: &ViewColumns, at| seen(at), &charge),
                |entry| {
                    visited.push(entry.key);
                },
            );
            visited.sort_unstable();
            assert_eq!(visited, expected, "read {read}");
        }
    }

    // Two teams' heroes and creeps together at the origin, and a third team's creep 100 m
    // away: from the origin with a reach of 1 m, the grids give only the bodies at the origin
    // of the classes a filter selects, each a step. Team 0's `allies:hero` selects its two
    // heroes and team 1's one, allied, 3 steps; `enemies:creep` from team 0 selects the far
    // creep's class alone, which gives nothing near, 0 steps; `all:tower` selects no class.
    let [none, hero, creep, _] = tags.sets;
    let mut more = IdAllocator::default();
    let units = [(0, hero), (0, hero), (0, creep), (1, hero), (1, none)]
        .map(|(team, tags)| row(more.allocate(), team, tags, [0, 0], 3));
    let mut units = units.to_vec();
    units.push(row(more.allocate(), 2, creep, [1000, 0], 3));
    let rows = rows(units);
    index.forget();
    for (text, steps, found) in [
        ("allies:hero", 3, vec![0, 1, 3]),
        ("enemies:creep", 0, vec![]),
        ("all:tower", 0, vec![]),
    ] {
        let filter = tags.filter(text);
        let query = TargetQuery {
            team: Team::new(0),
            from: Vec3::ZERO,
            shape: Shape::POINT,
            radius: Num::ONE,
            filter: &filter,
        };
        charge.clear();
        let rows_found = index.find(&rows, query, (|_: &ViewColumns, _| true, &charge));
        assert_eq!(rows_found, found, "{text}");
        assert_eq!(charge.get(), steps, "{text}");
    }
}
