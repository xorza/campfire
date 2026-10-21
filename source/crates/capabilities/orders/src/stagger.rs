use campfire_common::{Tick, Ticks};
use campfire_sim::StableId;

/// The tick a unit, `id`, of a turn each `period` is due in, as the tick `now` starts: `next`, or
/// before its first turn, the first tick from `now` that leaves the remainder of its stable id
/// when divided by the period, so the units of a period spread over it (04-capabilities/control.md,
/// AI thinks in staggered turns).
pub(crate) const fn due_at(next: Option<Tick>, id: StableId, period: Ticks, now: Tick) -> Tick {
    if let Some(next) = next {
        return next;
    }
    let period = period.get();
    let wait = (id.get() % period + period - now.get() % period) % period;
    Tick::new(now.get() + wait)
}
