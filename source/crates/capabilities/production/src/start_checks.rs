use bevy_ecs::query::{Allow, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_actions::{
    ActionBook, ActionCheck, ActionKind, ActionSlots, ActionTarget, Caster, Purse, SlotAim,
    SlotTimes, StartChecked,
};
use campfire_common::Tick;
use campfire_players::{PlayerResources, PlayerRows};
use campfire_sim::{EntityIndex, Position, StableId, Unpredicted};
use campfire_stats::{PlayerModifiers, PoolBook, UnitPoolsReadOnly};
use campfire_units::{Dead, Owner};
use campfire_values::{Motion, Relation, UnitTags, UnitType};

use crate::held_places::HeldPlaces;
use crate::holdings::Holdings;
use crate::production_books::{ProductionBooks, TrainAdmission, TrainAsk};
use crate::train_queue::TrainQueue;
use crate::trains::{Queues, ResearchCheck};

/// What tells whether a unit may start the train, the research or the build one of its slots
/// holds: the
/// actions, the production books and the holdings of a match with production, the players'
/// resources and modifiers, the life pool, and the living units (03-game-scripting.md, A bot reads
/// what its units may start).
#[derive(SystemParam, Debug)]
pub struct StartChecks<'w, 's> {
    index: Res<'w, EntityIndex>,
    actions: Res<'w, ActionBook>,
    books: Option<ProductionBooks<'w>>,
    holdings: Option<Res<'w, Holdings>>,
    resources: PlayerRows<'w, 's, PlayerResources>,
    modifiers: PlayerRows<'w, 's, PlayerModifiers>,
    life: Option<Res<'w, PoolBook>>,
    units: Query<
        'w,
        's,
        (
            &'static Position,
            &'static UnitType,
            &'static ActionSlots,
            &'static SlotTimes,
            UnitPoolsReadOnly,
            Option<&'static Owner>,
            Option<&'static UnitTags>,
            Option<&'static TrainQueue>,
            Option<&'static HeldPlaces>,
        ),
        (Without<Dead>, Allow<Unpredicted>),
    >,
    queues: Query<'w, 's, Queues, Allow<Unpredicted>>,
}

impl StartChecks<'_, '_> {
    /// Whether the unit `id` may start, in tick `now`, the train, the research or the build its
    /// slot `slot` holds, as the order's own checks find it as it applies, by the holdings as the
    /// last tick left them: its slot is ready and charged, its pools and its player afford it, its
    /// tags do not block it and its player meets its `requires`; a train or a research also finds
    /// room in its unit's queue, a train with supply room under its player's cap and, at a place,
    /// a free place, and a research its modifier neither held nor queued. A build's place is its
    /// order's, which this tests not. False for any other action, a unit that is gone, a match
    /// with no production, and one before its first tick, whose holdings count no unit yet.
    pub fn may_start(&self, id: StableId, slot: u8, now: Tick) -> bool {
        let holdings = self
            .holdings
            .as_deref()
            .filter(|holdings| holdings.followed());
        let (Some(books), Some(holdings)) = (&self.books, holdings) else {
            return false;
        };
        let Some(entity) = self.index.get(id) else {
            return false;
        };
        let Ok((at, &unit_type, slots, times, pools, owner, tags, queue, places)) =
            self.units.get(entity)
        else {
            return false;
        };
        let held = slots.slot(slot).and_then(|held| held.action);
        let Some(action) = held.and_then(|action| self.actions.get(action)) else {
            return false;
        };
        let target = match action.kind {
            ActionKind::Train | ActionKind::Research => ActionTarget::None,
            ActionKind::Build => ActionTarget::Point(at.get()),
            _ => return false,
        };
        let life = self.life.as_deref().and_then(PoolBook::life);
        let check = ActionCheck {
            player: None,
            book: &self.actions,
            now,
            caster: Caster::of(tags, Motion::Own, None),
            slots,
            purse: Purse::of(pools.of(life), self.resources.of(owner)),
            aim: SlotAim { slot, target },
            relation: |_| Relation::Friendly,
            living: |_| None,
        };
        let Some(StartChecked { checked, .. }) = times.start(check, None) else {
            return false;
        };
        let player_slot = owner.map(|owner| owner.player_slot());
        let modifiers = self.modifiers.of(owner);
        match action.kind {
            ActionKind::Research
                if books.researches.get(checked.id).is_some_and(|&spec| {
                    let check = ResearchCheck {
                        players: &self.modifiers,
                        researches: &books.researches,
                        upgrades: books.upgrades.as_deref(),
                        queues: &self.queues,
                    };
                    check.refuses(spec, id, entity)
                }) =>
            {
                false
            }
            ActionKind::Train | ActionKind::Research => queue.is_some_and(|queue| {
                let ask = TrainAsk {
                    unit_type,
                    queue,
                    owner: player_slot,
                    modifiers,
                    action: checked.id,
                    places,
                };
                books.admits(holdings, ask) != TrainAdmission::Refused
            }),
            _ => holdings.meet(player_slot, modifiers, books.requirements.of(checked.id)),
        }
    }
}
