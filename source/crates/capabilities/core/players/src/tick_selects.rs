use std::mem;
use std::ops::Range;

use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_common::PlayerSlot;
use campfire_declarations::{Role, Scratch};
use campfire_sim::{Addressee, EntityIndex, StableId, TickInputs};
use campfire_units::Owner;

use crate::commanded_units::CommandedUnits;
use crate::player_rows::{PlayerRows, PlayerRowsMut};
use crate::select::{Select, Selecting};

/// The units each player's orders of the tick command where its selects of the tick changed them
/// (04-capabilities/control.md, An order goes to the units commanded at its place): a copy for each
/// order that follows a change, which that order's units cost anyway, and none for a select no
/// order reads, so a tick holds no more than its orders reach. The tick's start fills it, and the
/// close of Inputs gives each player the units of its last select and empties it.
#[derive(Resource, Debug, Default)]
pub struct TickSelects {
    /// The units of each order that follows a change, from its place on, by player slot, then
    /// place.
    orders: Vec<CommandedAt>,
    /// The units each player that selected this tick commands at its end.
    last: Vec<LastSelected>,
    units: Vec<StableId>,
}

/// The units a player commands from a place among the tick's commands on.
#[derive(Debug)]
struct CommandedAt {
    player_slot: PlayerSlot,
    place: usize,
    units: Range<usize>,
}

/// The units a player's last select of the tick left.
#[derive(Debug)]
struct LastSelected {
    player_slot: PlayerSlot,
    units: Range<usize>,
}

/// What the resolve of a tick's selects keeps from tick to tick, so it allocates once: each
/// player's units as the tick's selects change them, and the buffers of a select's names and of
/// a merge.
#[derive(Debug, Default)]
pub(crate) struct SelectWork {
    players: Vec<PlayerWork>,
    named: Vec<StableId>,
    merged: Vec<StableId>,
}

/// A player's units as the tick's selects change them: whether a select of the tick touched them,
/// and whether an order read them since the last change.
#[derive(Debug)]
struct PlayerWork {
    player_slot: PlayerSlot,
    units: Vec<StableId>,
    touched: bool,
    unread: bool,
}

impl TickSelects {
    /// The units player `player_slot` commands at `place` among the tick's commands, where a
    /// select of the tick before it changed them; none where none did.
    pub(crate) fn at(&self, player_slot: PlayerSlot, place: usize) -> Option<&[StableId]> {
        let before = self
            .orders
            .partition_point(|at| (at.player_slot, at.place) <= (player_slot, place));
        let at = self.orders[..before]
            .last()
            .filter(|at| at.player_slot == player_slot)?;
        Some(&self.units[at.units.clone()])
    }

    /// Resolves the tick's selects in the order of its commands: each keeps, in increasing stable
    /// id, each unit it names that the match holds and that its player controls, a dead one among
    /// them, in place of what its player commanded, or beside it for a select that continues one
    /// (04-capabilities/control.md, A select holds what its player controls); and each command to a
    /// capability that follows a change takes a copy of its player's units. A body that is no
    /// select is passed over.
    pub(crate) fn resolve(
        inputs: Res<'_, TickInputs>,
        mut selects: ResMut<'_, Self>,
        commanded: PlayerRows<'_, '_, CommandedUnits>,
        index: Res<'_, EntityIndex>,
        owners: Query<'_, '_, &Owner>,
        mut work: Local<'_, SelectWork>,
    ) {
        let SelectWork {
            players,
            named,
            merged,
        } = &mut *work;
        for player in players.iter_mut() {
            player.touched = false;
        }
        let selects = &mut *selects;
        for (to, command) in inputs.all() {
            let player_slot = command.player_slot;
            let Addressee::Players = to else {
                let player = players
                    .iter_mut()
                    .find(|player| player.player_slot == player_slot && player.touched);
                if let Some(player) = player.filter(|player| player.unread) {
                    player.unread = false;
                    let units = selects.copy(player);
                    selects.orders.push(CommandedAt {
                        player_slot,
                        place: command.place,
                        units,
                    });
                }
                continue;
            };
            named.clear();
            let Some(selecting) = Select::decode_into(command.body, named) else {
                continue;
            };
            // Only a select that continues one reads what its player commanded before the tick.
            let held = commanded
                .of_player_slot(player_slot)
                .map(CommandedUnits::get);
            let base = held.filter(|_| selecting == Selecting::More);
            let player = PlayerWork::touch(players, player_slot, base);
            let controls = |unit: &StableId| {
                let owner = index.get(*unit).and_then(|entity| owners.get(entity).ok());
                owner.is_some_and(|owner| owner.player_slot() == player_slot)
            };
            let kept = named.iter().copied().filter(controls);
            match selecting {
                Selecting::New => {
                    player.units.clear();
                    player.units.extend(kept);
                }
                Selecting::More => {
                    merge(&player.units, kept, merged);
                    mem::swap(&mut player.units, merged);
                }
            }
            player.unread = true;
        }
        for player in players.iter().filter(|player| player.touched) {
            let units = selects.copy(player);
            selects.last.push(LastSelected {
                player_slot: player.player_slot,
                units,
            });
        }
        selects
            .orders
            .sort_unstable_by_key(|at| (at.player_slot, at.place));
    }

    /// Gives each player that selected this tick the units of its last select, as Inputs closes,
    /// and empties the tick's selects; a player of no entity, as a test's match may hold, keeps
    /// none past its tick.
    pub(crate) fn commit(
        mut selects: ResMut<'_, Self>,
        mut commanded: PlayerRowsMut<'_, '_, CommandedUnits>,
    ) {
        for last in &selects.last {
            let units = &selects.units[last.units.clone()];
            let Some(mut held) = commanded.of_player_slot_mut(last.player_slot) else {
                continue;
            };
            if held.get() != units {
                held.set(units);
            }
        }
        selects.orders.clear();
        selects.last.clear();
        selects.units.clear();
    }

    /// A copy of `player`'s units, at its place among the tick's units.
    fn copy(&mut self, player: &PlayerWork) -> Range<usize> {
        let start = self.units.len();
        self.units.extend_from_slice(&player.units);
        start..self.units.len()
    }
}

impl PlayerWork {
    /// The work of `player_slot` among `players`, touched by a select of the tick: on its first
    /// touch, the units `base` gives, or none.
    fn touch<'w>(
        players: &'w mut Vec<Self>,
        player_slot: PlayerSlot,
        base: Option<&[StableId]>,
    ) -> &'w mut Self {
        let at = players
            .iter()
            .position(|player| player.player_slot == player_slot)
            .unwrap_or_else(|| {
                players.push(Self {
                    player_slot,
                    units: Vec::new(),
                    touched: false,
                    unread: false,
                });
                players.len() - 1
            });
        let player = &mut players[at];
        if !player.touched {
            player.touched = true;
            player.unread = false;
            player.units.clear();
            player.units.extend_from_slice(base.unwrap_or_default());
        }
        player
    }
}

/// Writes into `out` the units of `held` and of `more`, both rising, in increasing stable id, each
/// once.
fn merge(held: &[StableId], more: impl Iterator<Item = StableId>, out: &mut Vec<StableId>) {
    out.clear();
    let mut held = held.iter().copied().peekable();
    for unit in more {
        while let Some(before) = held.next_if(|&before| before < unit) {
            out.push(before);
        }
        let _ = held.next_if_eq(&unit);
        out.push(unit);
    }
    out.extend(held);
}

impl Scratch for TickSelects {
    fn installed(_: &Role) -> Option<Self> {
        Some(Self::default())
    }

    fn is_clear(&self) -> bool {
        self.orders.is_empty() && self.last.is_empty() && self.units.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;

    use super::*;

    fn ids(values: &[u8]) -> Vec<StableId> {
        values
            .iter()
            .map(|&value| Binary::decode(&[value]).unwrap())
            .collect()
    }

    #[test]
    fn a_merge_keeps_each_unit_of_both_once_in_increasing_stable_id() {
        // 1, 3 and 5 held, 2, 3 and 6 more: 3 once, the rest between; either side empty gives the
        // other.
        let mut out = ids(&[9]);
        merge(&ids(&[1, 3, 5]), ids(&[2, 3, 6]).into_iter(), &mut out);
        assert_eq!(out, ids(&[1, 2, 3, 5, 6]));
        merge(&[], ids(&[4]).into_iter(), &mut out);
        assert_eq!(out, ids(&[4]));
        merge(&ids(&[4, 7]), [].into_iter(), &mut out);
        assert_eq!(out, ids(&[4, 7]));
    }
}
