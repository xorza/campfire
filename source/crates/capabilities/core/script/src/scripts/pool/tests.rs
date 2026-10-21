use campfire_common::Tick;
use campfire_sim::IdAllocator;
use campfire_values::{ActionId, ModifierId, PackageIndex, Rank};

use super::*;
use crate::scripts::call_start::Spent;
use crate::scripts::mode_scripts::ListPlace;

#[test]
fn a_call_draws_from_its_payers_pool_and_only_a_call_no_player_causes_waits() {
    let now = Tick::new(3);
    let unit = IdAllocator::default().allocate();
    let slot = Seat::new(1);
    let package = PackageIndex::MODE;
    let cast = CallStart::cast(ActionId::new(0), Rank::FIRST, unit, package, now);
    let hook = CallStart::hook(ModifierId::new(0), package, 0, now);
    let mode = CallStart::mode(ScriptRole::Mode, now);
    let paid = |start: CallStart| CallStart {
        payer: Some(slot),
        ..start
    };
    let sourced = CallStart {
        acting: Some(unit),
        ..hook
    };
    let cases = [
        // A cast by a player's unit, and one by a unit no player controls or that is gone.
        (paid(cast), Some(Pool::Player(slot)), Spent::Fails),
        (cast, Some(Pool::Think), Spent::Fails),
        // A modifier's hook from a player's unit, from another unit, and from no source.
        (paid(sourced), Some(Pool::Player(slot)), Spent::Fails),
        (sourced, Some(Pool::Think), Spent::Fails),
        (hook, Some(Pool::Mode), Spent::Fails),
        // The mode's event, and its answer to a player's input.
        (mode, Some(Pool::Mode), Spent::Waits),
        (paid(mode), Some(Pool::Player(slot)), Spent::Fails),
        // A think, of any unit's, and a pure hook, which draws from none.
        (CallStart::think(unit, now), Some(Pool::Think), Spent::Waits),
        (
            paid(CallStart::think(unit, now)),
            Some(Pool::Think),
            Spent::Waits,
        ),
        (CallStart::pure(ListPlace::FIRST, now), None, Spent::Waits),
    ];
    for (start, pool, spent) in cases {
        assert_eq!(
            (Pool::of_call(&start), start.when_spent()),
            (pool, spent),
            "{start:?}"
        );
    }
}
