use std::num::NonZeroU16;

use campfire_actions::{ActionProgress, ActionSlots, InProgress, SlotTimes};
use campfire_client_files::AttackPhase;
use campfire_combat::{ClipShots, Weapon};
use campfire_common::Tick;
use campfire_declarations::ById;
use campfire_values::ActionId;

/// What a drawing keeps of its unit's slots between the frames that read them: each slot's ready
/// tick as the client last saw it, and the tick the client saw it rise in, the tick of the slot's
/// last shot, as no state the client holds says when a shot was (02-engine-core.md, The client
/// derives an attack's phase).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SlotShots {
    ready: Vec<Tick>,
    fired: Vec<Option<Tick>>,
}

/// What an attack's phase reads of a unit at the tick `now`: its progress and its slots, its
/// slots' times and its clips' shots where its client holds them, and the book of weapons.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AttackParts<'a> {
    pub(crate) progress: &'a ActionProgress,
    pub(crate) slots: &'a ActionSlots,
    pub(crate) times: Option<&'a SlotTimes>,
    pub(crate) clips: Option<&'a ClipShots>,
    pub(crate) weapons: &'a ById<ActionId, Weapon>,
    pub(crate) now: Tick,
}

/// What an attack's phase reads of one slot at the tick `now`: whether its unit attacks; whether
/// it strikes, its attack under way or its shot in this tick; the end of its pre-attack no shot
/// spent; the tick it is ready again, where its client holds it; whether its weapon fires in
/// clips and its clip is full, so its last shot emptied it; and the shots of its clip and those
/// it fired since its clip was last full, where its client holds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotAttack {
    pub(crate) attacking: bool,
    pub(crate) striking: bool,
    pub(crate) pre_attack_until: Option<Tick>,
    pub(crate) ready_at: Option<Tick>,
    pub(crate) reloads: bool,
    pub(crate) clip_shots: Option<NonZeroU16>,
    pub(crate) fired: u16,
    pub(crate) now: Tick,
}

impl SlotShots {
    /// Takes the slots' ready ticks `ready` at `now`: a slot whose ready tick rose fired at `now`.
    /// The first ticks it takes, or those of other slots, fired none.
    pub(crate) fn see(&mut self, ready: impl ExactSizeIterator<Item = Tick>, now: Tick) {
        if self.ready.len() != ready.len() {
            self.ready.clear();
            self.ready.extend(ready);
            self.fired.clear();
            self.fired.resize(self.ready.len(), None);
            return;
        }
        let seen = self.ready.iter_mut().zip(&mut self.fired);
        for ((held, fired), ready_at) in seen.zip(ready) {
            if ready_at > *held {
                *fired = Some(now);
            }
            *held = ready_at;
        }
    }

    /// The tick the client last saw `slot` fire in; none before.
    fn fired(&self, slot: u8) -> Option<Tick> {
        self.fired.get(usize::from(slot)).copied().flatten()
    }
}

impl AttackParts<'_> {
    /// What the phase of `slot` reads of the unit, its shots `shots` seen.
    pub(crate) fn slot(&self, shots: &SlotShots, slot: u8) -> SlotAttack {
        let under_way = matches!(
            self.progress.in_progress(),
            Some(InProgress::Attack { slot: of, .. }) if of == slot
        );
        let pre_attack = self.progress.pre_attack();
        let pre_attack = pre_attack.filter(|pre| pre.slot == slot && !pre.spent);
        let held = self.slots.slot(slot);
        let action = held.and_then(|held| held.action);
        let weapon = action.and_then(|action| self.weapons.get(action));
        let clipped = weapon.is_some_and(Weapon::has_clip);
        let full = action
            .zip(self.clips)
            .is_some_and(|(action, clips)| clips.is_full(action));
        let rank = held.and_then(|held| held.rank);
        let clip_shots = weapon
            .zip(rank)
            .and_then(|(weapon, rank)| weapon.clip_shots(rank));
        let fired = action
            .zip(self.clips)
            .map_or(0, |(action, clips)| clips.fired(action));
        SlotAttack {
            attacking: self.progress.attack_target().is_some(),
            striking: under_way || shots.fired(slot) == Some(self.now),
            pre_attack_until: pre_attack.map(|pre| pre.done_at),
            ready_at: self
                .times
                .and_then(|times| times.iter().nth(usize::from(slot)))
                .map(|time| time.ready_at),
            reloads: clipped && full,
            clip_shots: clip_shots.filter(|_| self.clips.is_some()),
            fired,
            now: self.now,
        }
    }
}

/// The rounds the clip of a slot lacks, and whether it lacks them as it reloads, until its slot is
/// ready, so they come back with no change its client sees.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ClipLack {
    pub(crate) rounds: u16,
    pub(crate) reloads: bool,
}

impl SlotAttack {
    /// What its clip lacks, as Zero Hour gives its draw its clip's remaining ammo
    /// (`Object::adjustModelConditionForWeaponStatus`): every round while it reloads, until it is
    /// ready, else the shots it fired since its clip was last full; none for a weapon of no clip,
    /// or of a unit whose clips its client does not hold.
    pub(crate) fn lack(self) -> ClipLack {
        let Some(shots) = self.clip_shots else {
            return ClipLack::default();
        };
        let waits = self.ready_at.is_some_and(|ready_at| self.now < ready_at);
        if self.reloads && waits {
            return ClipLack {
                rounds: shots.get(),
                reloads: true,
            };
        }
        ClipLack {
            rounds: self.fired,
            reloads: false,
        }
    }

    /// Its phase, as `Object::updateDrawableWeaponState` sets its weapon's condition: none unless
    /// its unit attacks; its shot, which comes first; its pre-attack, until its end; the reload of
    /// its clip, until it is ready; the wait for its next shot until then; and none once it is
    /// ready.
    pub(crate) fn phase(self) -> Option<AttackPhase> {
        if !self.attacking {
            return None;
        }
        if self.striking {
            return Some(AttackPhase::Firing);
        }
        if self.pre_attack_until.is_some_and(|until| self.now < until) {
            return Some(AttackPhase::PreAttack);
        }
        let waits = self.ready_at.is_some_and(|ready_at| self.now < ready_at);
        waits.then_some(if self.reloads {
            AttackPhase::Reloading
        } else {
            AttackPhase::Between
        })
    }
}

#[cfg(test)]
mod tests;
