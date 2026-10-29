//! Manages the prediction history buffer, which stores past local predicted component states.
//!
//! The prediction history is used to:
//! 1. Compare local predicted values with confirmed values from the server to detect mismatches
//! 2. Rollback to a past local state and replay the simulation
use core::fmt::{self, Debug, Display};
use core::iter::FilterMap;

use bevy_ecs::component::ComponentIdFor;
use bevy_ecs::prelude::*;
use bevy_ecs::resource::IsResource;
use bevy_ecs::world::DeferredWorld;
use bevy_reflect::Reflect;
use bevy_replicon::client::confirm_history::ConfirmHistory;
use bevy_replicon::shared::replication::diff::Diffable;
use bevy_replicon::shared::replication::storage::ReplicationStorage;
use bevy_utils::prelude::DebugName;
use campfire_net_core::Iter as EntryIter;
use campfire_net_core::NetTick;
use campfire_net_core::TickBuffer;
use campfire_net_core::Ticked;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::ConfirmedTickMap;
use campfire_net_replication::DeferredEntityCommands;
use campfire_net_replication::HistoryComponent;
use campfire_net_replication::HistoryDiffReceiver;
use campfire_net_replication::HistoryState;
use campfire_net_replication::PreSpawned;
use campfire_net_replication::Predicted;
use campfire_net_sync::{InputTimelineSetup, SyncedLocalTimeline};
use campfire_net_timeline::LocalTimeline;
use campfire_net_timeline::LocalTimelineShift;
use tracing::trace;

use crate::locally_predicted::LocallyPredicted;
use crate::prediction_setup::PredictionSetup;

/// Holds the history of locally predicted component states, to allow for rollback.
///
/// This stores only local prediction samples. Authoritative samples from the
/// remote are stored separately in [`ConfirmedHistory`].
///
/// The values stay ordered from oldest (front) to most recent (back), and only the ticks where the
/// value changed hold an entry: a lookup takes the newest entry at or before the tick.
#[derive(Component, Debug, Reflect)]
pub struct PredictionHistory<C> {
    buffer: TickBuffer<HistoryState<C>>,
}

impl<C> Default for PredictionHistory<C> {
    fn default() -> Self {
        Self {
            buffer: TickBuffer::default(),
        }
    }
}

impl<C> PredictionHistory<C> {
    /// Iterates the present values from oldest to most recent.
    pub fn iter(&self) -> Iter<'_, C> {
        self.buffer.iter().filter_map(|entry| match entry.value {
            HistoryState::Updated(value) => Some(Ticked::new(entry.tick, value)),
            HistoryState::Removed => None,
        })
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// The value at the specified tick.
    pub fn get(&self, tick: NetTick) -> Option<&C> {
        self.get_state_at_or_before(tick)?.value()
    }

    /// The full state at the specified tick.
    pub fn get_state_at_or_before(&self, tick: NetTick) -> Option<&HistoryState<C>> {
        let index = self.buffer.index_at_or_before(tick)?;
        self.buffer.get(index).map(|entry| entry.value)
    }

    /// Reset the history for this value
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Clear states older than the specified tick while keeping the effective
    /// state at `tick` as the new oldest anchor.
    pub fn clear_until_tick(&mut self, tick: NetTick) {
        let Some(anchor) = self.buffer.index_at_or_before(tick) else {
            return;
        };
        // The state at or before `tick` stays, as it resolves the gap until the next entry.
        self.buffer.drain_front(anchor);
        self.buffer.retick_front(tick);
    }

    /// Add a state to the history, replacing one at the same tick.
    /// The tick must not be older than the most recent update in the history: the lookups rely
    /// on the order.
    pub fn add_state(&mut self, tick: NetTick, state: HistoryState<C>) {
        self.buffer.push(tick, state);
    }

    /// Add a value to the history, or the component's absence, as [`Self::add_state`] does.
    pub fn add(&mut self, tick: NetTick, value: Option<C>) {
        self.add_state(
            tick,
            match value {
                Some(value) => HistoryState::Updated(value),
                None => HistoryState::Removed,
            },
        );
    }

    /// Shift every tick by `delta`, as the local timeline does. Entries that would saturate at
    /// tick 0 together keep only the newest.
    pub fn shift(&mut self, delta: i32) {
        self.buffer.shift(delta, |_, _| {});
    }

    /// Every entry, present or removed, from oldest to most recent.
    pub fn states(&self) -> EntryIter<'_, HistoryState<C>> {
        self.buffer.iter()
    }

    /// Clear all states strictly newer than the specified tick.
    pub fn clear_after_tick(&mut self, tick: NetTick) {
        self.buffer.truncate_after(tick);
    }
}

/// The iterator contains the elements that are actually present in the history
/// from the oldest to the most recent
impl<'a, C> IntoIterator for &'a PredictionHistory<C> {
    type Item = Ticked<&'a C>;
    type IntoIter = Iter<'a, C>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over the present values of a [`PredictionHistory`], from oldest to most recent.
pub type Iter<'a, C> = FilterMap<
    EntryIter<'a, HistoryState<C>>,
    fn(Ticked<&'a HistoryState<C>>) -> Option<Ticked<&'a C>>,
>;

impl<C: Debug> Display for PredictionHistory<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PredictionHistory[")?;
        for (i, Ticked { tick, value: state }) in self.states().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            let state_char = match state {
                HistoryState::Updated(_) => "P",
                HistoryState::Removed => "R",
            };
            write!(f, "{tick:?}:{state_char}")?;
        }
        write!(f, "]")
    }
}

impl<T: Component + Clone> PredictionHistory<T> {
    /// Store every update on the predicted entity in the [`PredictionHistory`], of the entities
    /// whose component changed alone, as change detection tells in a rollback's replay too.
    ///
    /// [`SyncedLocalTimeline`] skips this system until timeline synchronization has completed, so
    /// pre-sync component values are never recorded under invalid local tick labels. This system
    /// only handles changes; removals are handled by [`PredictionHistory::record_removal`], and
    /// the cut to the rollback horizon by [`PredictionHistory::prune_predicted`].
    pub(crate) fn update(
        mut query: Query<'_, '_, (Entity, &T, &mut Self), Changed<T>>,
        timeline: SyncedLocalTimeline<'_, '_>,
    ) {
        // tick for which we will record the history (either the current client tick or the current
        // rollback tick)
        let tick = timeline.tick();
        for (entity, component, mut history) in query.iter_mut() {
            history.add(tick, Some(component.clone()));
            // Structured per-entity snapshot — `entity` is included so queries
            // against the JSONL can segment history growth/reset by entity
            // (e.g. to tell a locally predicted entity's history apart from a
            // just-arrived replicated player's history).
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "prediction_history_predicted",
                entity = ?entity,
                component = ?DebugName::type_name::<T>(),
                local_tick = tick.0,
                history_len = history.len(),
                "recorded predicted component history"
            );
        }
    }

    /// Prunes the predicted history of every entity to the rollback horizon, once a frame in
    /// `PostUpdate`, after the fixed loop recorded it: a cut at a later tick drains what the cuts
    /// of each tick before it would have. A cut is no update, so it marks no history changed.
    pub(crate) fn prune_predicted(
        setup: Res<'_, PredictionSetup>,
        input_setup: Res<'_, InputTimelineSetup>,
        mut query: Query<'_, '_, &mut Self>,
        timeline: SyncedLocalTimeline<'_, '_>,
    ) {
        let oldest_rollback_tick = setup.oldest_rollback_tick(timeline.tick(), &input_setup);
        for mut history in query.iter_mut() {
            history
                .bypass_change_detection()
                .clear_until_tick(oldest_rollback_tick);
        }
    }

    /// Prune authoritative history on predicted entities to the rollback horizon.
    ///
    /// Runs once per frame in `PostUpdate`: after the fixed loop has recorded history
    /// and after the rollback check for the current frame, so snapshots received in
    /// `PreUpdate` remain visible to the rollback decision, including forced
    /// rollbacks.
    ///
    /// The cut never passes the latest completed confirmed tick: `clear_until_tick` stores
    /// the state at the cut as authoritative there, which past that confirmed tick would be a
    /// stale value at a tick the server has not reached.
    pub(crate) fn prune_confirmed(
        setup: Res<'_, PredictionSetup>,
        input_setup: Res<'_, InputTimelineSetup>,
        confirmed_ticks: Res<'_, ConfirmedTickMap>,
        mut query: Query<'_, '_, &mut ConfirmedHistory<T>, With<Self>>,
        timeline: SyncedLocalTimeline<'_, '_>,
    ) {
        let Some(confirmed_tick) = confirmed_ticks.last_confirmed_tick() else {
            return;
        };
        let oldest_rollback_tick = setup.oldest_rollback_tick(timeline.tick(), &input_setup);
        let cut = oldest_rollback_tick.min(confirmed_tick);

        // A cut is not an authoritative update, which is what `Changed` means for the history.
        for mut history in query.iter_mut() {
            history.bypass_change_detection().clear_until_tick(cut);
        }
    }
}

impl<C: Component> PredictionHistory<C> {
    /// Shift locally indexed prediction history when the local simulation clock jumps.
    pub(crate) fn shift_with_local_timeline(
        trigger: On<'_, '_, LocalTimelineShift>,
        mut query: Query<'_, '_, &mut Self>,
    ) {
        for mut history in query.iter_mut() {
            history.shift(trigger.delta);
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "prediction_history_tick_delta",
                component = ?DebugName::type_name::<C>(),
                tick_span = trigger.delta,
                history_len = history.len(),
                "shifted prediction history ticks"
            );
        }
    }

    /// If a predicted component is removed on the [`Predicted`] entity, add the removal to the
    /// history. [`SyncedLocalTimeline`] skips this observer before timeline synchronization has
    /// completed.
    pub(crate) fn record_removal(
        trigger: On<'_, '_, Remove<C>>,
        mut predicted_query: Query<'_, '_, &mut Self>,
        timeline: SyncedLocalTimeline<'_, '_>,
    ) {
        let tick = timeline.tick();
        if let Ok(mut history) = predicted_query.get_mut(trigger.entity) {
            history.add(tick, None);
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "prediction_history_removed",
                entity = ?trigger.entity,
                component = ?DebugName::type_name::<C>(),
                local_tick = tick.0,
                history_len = history.len(),
                "recorded predicted component removal"
            );
        }
    }
}

impl<C: Component + Clone> PredictionHistory<C> {
    /// When `C` or one of [`Predicted`], [`PreSpawned`] or [`LocallyPredicted`] is added to an
    /// entity, ensure [`PredictionHistory<C>`] is present for predicted entities and resource
    /// entities. [`IsResource`] is an eligibility check rather than a trigger: `Add<C>` already
    /// fires when a resource is inserted, and triggering on every `IsResource` addition would wake
    /// this observer for unrelated resource types.
    pub(crate) fn add_on_prediction(
        trigger: On<'_, '_, Add<(C, Predicted, PreSpawned, LocallyPredicted)>>,
        mut world: DeferredWorld<'_>,
    ) {
        let entity = trigger.entity;
        let Ok(found) = world.get_entity(entity) else {
            return;
        };
        if !(found.contains::<C>() && (is_predicted(found) || found.contains::<IsResource>())) {
            return;
        }
        trace!(
            target: "campfire_net_debug::prediction",
            kind = "prediction_history_insert",
            ?entity,
            component = ?DebugName::type_name::<C>(),
            "inserted prediction history component"
        );
        world.commands().queue(move |world: &mut World| {
            if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                entity_mut.insert_if_new(Self::default());
            }
        });
    }
}

impl<C: HistoryComponent> PredictionHistory<C> {
    /// Seeds the last replicated value when [`Predicted`] is added manually to an
    /// already-replicated entity on the receiver.
    ///
    /// Initial replication through `PredictedSend` also adds the receiver-side [`Predicted`]
    /// marker, but its marker writer adds [`ConfirmedHistory<C>`] in the same replication command.
    /// The [`Without<ConfirmedHistory<C>>`] query filter therefore makes that path a no-op here.
    ///
    /// Without this baseline an unchanged authoritative component might never
    /// receive another update, leaving state-based prediction unable to compare it
    /// at later completed server ticks.
    pub(crate) fn backfill_confirmed(
        trigger: On<'_, '_, Add<Predicted>>,
        component_id: ComponentIdFor<'_, C>,
        mut world: DeferredWorld<'_>,
    ) {
        // A component added together with Predicted is a new local value, not a
        // previously replicated authoritative baseline.
        if trigger.trigger().components.contains(&component_id.get()) {
            return;
        }
        let entity = trigger.entity;
        let Ok(found) = world.get_entity(entity) else {
            return;
        };
        if found.contains::<ConfirmedHistory<C>>() {
            return;
        }
        let (Some(component), Some(confirm_history)) =
            (found.get::<C>(), found.get::<ConfirmHistory>())
        else {
            return;
        };
        let Some(tick) = world
            .get_resource::<ConfirmedTickMap>()
            .and_then(|confirmed_ticks| confirmed_ticks.confirmed_state_tick(confirm_history))
        else {
            return;
        };
        let history = ConfirmedHistory::seed(tick, component.clone());
        trace!(
            ?entity,
            ?tick,
            component = ?DebugName::type_name::<C>(),
            "backfilled confirmed history for late prediction opt-in"
        );
        world.commands().entity(entity).insert(history);
    }

    /// During a replay, sets the component to its confirmed state at the end of the previous
    /// tick, where the confirmed history holds one, in place of the predicted value.
    ///
    /// [`PredictionSystems::SnapToConfirmed`](crate::prediction_systems::PredictionSystems::SnapToConfirmed)
    /// gates this system with [`Rollback::is_active`](campfire_net_timeline::Rollback::is_active),
    /// so the global [`Rollback`](campfire_net_timeline::Rollback) resource does not need to be
    /// fetched by every monomorphized component system.
    pub(crate) fn snap_to_confirmed(
        timeline: Option<Res<'_, LocalTimeline>>,
        mut query: Query<'_, '_, (Entity, Option<&mut C>, &ConfirmedHistory<C>), With<Predicted>>,
        mut deferred: Deferred<'_, DeferredEntityCommands>,
    ) {
        let Some(timeline) = timeline else {
            return;
        };
        let tick = timeline.tick();
        for (entity_id, mut live, history) in &mut query {
            // The confirmed value at the end of the previous tick is the state this tick starts
            // from: snapping to this tick's own value would simulate its change twice.
            if let Some(confirmed_state) = history.get_state_at(tick - 1) {
                match confirmed_state {
                    HistoryState::Updated(confirmed_value) => {
                        if let Some(current) = &mut live {
                            if **current != *confirmed_value {
                                trace!(
                                    target: "campfire_net_debug::prediction",
                                    kind = "PredictionHistory::snap_to_confirmed",
                                    entity = ?entity_id,
                                    component = ?DebugName::type_name::<C>(),
                                    local_tick = tick.0,
                                    confirmed_tick = (tick - 1).0,
                                    value = ?confirmed_value,
                                    "snapped predicted component to confirmed value during rollback"
                                );
                                **current = confirmed_value.clone();
                            }
                        } else {
                            trace!(
                                target: "campfire_net_debug::prediction",
                                kind = "snap_to_confirmed_insert",
                                entity = ?entity_id,
                                component = ?DebugName::type_name::<C>(),
                                local_tick = tick.0,
                                confirmed_tick = (tick - 1).0,
                                value = ?confirmed_value,
                                "inserted confirmed component during rollback"
                            );
                            deferred.insert(entity_id, confirmed_value.clone());
                        }
                    }
                    HistoryState::Removed => {
                        if live.is_some() {
                            trace!(
                                target: "campfire_net_debug::prediction",
                                kind = "snap_to_confirmed_remove",
                                entity = ?entity_id,
                                component = ?DebugName::type_name::<C>(),
                                local_tick = tick.0,
                                confirmed_tick = (tick - 1).0,
                                "removed component for confirmed removal during rollback"
                            );
                            deferred.remove::<C>(entity_id);
                        }
                    }
                }
            }
        }
    }
}

impl<C: HistoryComponent + Diffable> PredictionHistory<C> {
    pub(crate) fn add_diff_receiver(
        trigger: On<'_, '_, Add<(C, Predicted, PreSpawned, LocallyPredicted)>>,
        mut world: DeferredWorld<'_>,
    ) {
        let entity = trigger.entity;
        let Ok(found) = world.get_entity(entity) else {
            return;
        };
        if !(found.contains::<C>() && is_predicted(found)) {
            return;
        }
        world.commands().queue(move |world: &mut World| {
            let Some(tick) = world
                .get::<ConfirmHistory>(entity)
                .and_then(|confirm_history| {
                    world
                        .resource::<ConfirmedTickMap>()
                        .confirmed_state_tick(confirm_history)
                })
            else {
                return;
            };
            if let Some(mut storage) = world.get_resource_mut::<ReplicationStorage>() {
                HistoryDiffReceiver::<C>::seed(&mut storage, entity, tick);
            }
        });
    }
}

/// Whether `entity` is predicted, pre-spawned or locally predicted. An observer reads the entity
/// itself, which no app's disabling component hides, where a query would miss an entity an app
/// hides until its prediction starts.
fn is_predicted(entity: EntityRef<'_>) -> bool {
    entity.contains::<Predicted>()
        || entity.contains::<PreSpawned>()
        || entity.contains::<LocallyPredicted>()
}

#[cfg(test)]
mod tests;
