use super::*;

#[test]
fn a_drawing_that_leaves_a_ghost_stays_where_its_team_does_not_see_and_no_other() {
    // A fog of 2 × 2 cells of 1 m; a drawing at (1.5, 0.5), cell 1.
    let fog = MatchFog::of_cells(2, 2);
    let at = Vec3::new(1.5, 3.0, 0.5);
    let left = |leaves: bool, torn: bool| Left {
        leaves: leaves.then_some(&fog),
        torn,
        at,
    };
    assert_eq!(left(true, false).stays(), Stays::Ghost(Ghost { cell: 1 }));
    // A unit that does not leave one, or past the grid: gone.
    assert_eq!(left(false, false).stays(), Stays::Gone);
    let past = Left {
        at: Vec3::new(5.0, 0.0, 0.5),
        ..left(true, false)
    };
    assert_eq!(past.stays(), Stays::Gone);
    // A drawing torn off its unit outlives it, whatever it leaves.
    assert_eq!(left(true, true).stays(), Stays::Torn);
}
