use std::ops::Range;

use bevy_ecs::query::QueryState;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_players::Player;
use campfire_script::{CallError, CallExtension, CallStart, Commit};
use campfire_values::PerkId;

use crate::player_perks::PlayerPerks;
use crate::player_points::PlayerPoints;

/// Progression's part of a call: each player's points, none for a player of no track of points, and
/// its perks, by player slot, as the call reads them (09-zero-hour.md, A bot buys its skill set).
/// It reads them as each call begins, as an earlier call's grant changes the perks, through one
/// query of the players, its buffers kept, so a call allocates none once they hold every player.
#[derive(Debug, Default)]
pub struct ProgressionCall {
    players: Option<QueryState<PlayerParts>>,
    points: Vec<Option<u32>>,
    /// Every player's perks, in the order the query reads the players, each player's run of them
    /// at `runs[player]`.
    perks: Vec<PerkId>,
    runs: Vec<Range<u32>>,
}

/// The parts of a player the call reads.
type PlayerParts = (
    &'static Player,
    Option<&'static PlayerPoints>,
    Option<&'static PlayerPerks>,
);

impl ProgressionCall {
    /// The unspent points of player `player_slot`; none for a player the session lacks, or of no
    /// track of points.
    pub(crate) fn points(&self, player_slot: PlayerSlot) -> Option<u32> {
        self.points.get(player_slot.index()).copied().flatten()
    }

    /// Whether player `player_slot` has `perk`; false for a player the session lacks.
    pub(crate) fn has_perk(&self, player_slot: PlayerSlot, perk: PerkId) -> bool {
        let Some(run) = self.runs.get(player_slot.index()) else {
            return false;
        };
        let perks = &self.perks[run.start as usize..run.end as usize];
        perks.binary_search(&perk).is_ok()
    }

    /// The players' query, made at the first read, its archetypes as `world` holds them.
    fn players(&mut self, world: &World) -> &QueryState<PlayerParts> {
        let players = self.players.get_or_insert_with(|| {
            QueryState::try_new(world).expect("the install registers every declared component")
        });
        players.update_archetypes(world);
        players
    }
}

impl CallExtension for ProgressionCall {
    const COMMITS: Commit = Commit::BeforeEffects;

    fn begin_batch(&mut self, _: &World) {}

    /// Reads the players' points and perks, which an earlier call's effects of the batch change.
    fn begin(&mut self, world: &World, _: &CallStart) -> Result<(), CallError> {
        let (mut points, mut perks, mut runs) = (
            std::mem::take(&mut self.points),
            std::mem::take(&mut self.perks),
            std::mem::take(&mut self.runs),
        );
        perks.clear();
        let (mut count, mut player_slots) = (0, 0);
        for (player, held, has) in self.players(world).iter_manual(world) {
            let at = player.player_slot().index();
            if at >= points.len() {
                points.resize(at + 1, None);
                runs.resize(at + 1, 0..0);
            }
            points[at] = held.map(|held| held.get());
            let start = u32::try_from(perks.len()).expect("perks fit u32");
            perks.extend(has.map_or(&[][..], PlayerPerks::get));
            runs[at] = start..u32::try_from(perks.len()).expect("perks fit u32");
            count += 1;
            player_slots = usize::max(player_slots, at + 1);
        }
        debug_assert_eq!(
            count, player_slots,
            "the players hold the player slots from the first on"
        );
        points.truncate(count);
        runs.truncate(count);
        self.points = points;
        self.perks = perks;
        self.runs = runs;
        Ok(())
    }

    /// It writes nothing.
    fn commit(&mut self, _: &mut World) {}
}
