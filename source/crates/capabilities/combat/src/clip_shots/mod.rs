use std::num::{NonZeroU16, NonZeroU64};

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_actions::{ActionSlot, ActionSlots};
use campfire_common::Ticks;
use campfire_declarations::ById;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::ActionId;
use campfire_visibility::DataKind;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::attack_data::ClipRefill;
use crate::weapon::ClipRule;
use crate::weapon::Weapon;

/// The shots each weapon of a unit with a clip fired since its clip was last full, by action, in
/// the order of their ids: a weapon of none is not in it, so one state has one form, which its
/// owner's client predicts as it predicts its slots' times. Every unit with slots holds it, as
/// Bevy's required component, empty as the slots come. A weapon in two slots counts once, its
/// two slots firing from one clip.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ClipShots(Vec<Fired>);

/// A clip that refills at home: its weapon, its clip, and the shots it fired since it was full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HomeClip {
    pub action: ActionId,
    pub clip: ClipRule,
    pub fired: u16,
}

impl HomeClip {
    /// The ticks its reload takes to give back the shots it fired: its reload's share of them,
    /// rounded up.
    pub fn reload_ticks(&self) -> Ticks {
        let ticks = self.clip.reload.get() * u64::from(self.fired);
        Ticks::new(ticks.div_ceil(NonZeroU64::from(self.clip.shots).get()))
    }
}

/// How far a reload at home went: `passed` of its `total` ticks, of more than none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HomeReload {
    pub passed: Ticks,
    pub total: Ticks,
}

/// A weapon's shots since its clip was last full, at least one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Fired {
    action: ActionId,
    shots: u16,
}

impl ClipShots {
    /// Whether the clip of `action` is full: it fired no shot since its clip was last full.
    pub fn is_full(&self, action: ActionId) -> bool {
        self.0
            .binary_search_by_key(&action, |fired| fired.action)
            .is_err()
    }

    /// The shots `action` fired since its clip was last full, 0 for a full clip.
    pub fn fired(&self, action: ActionId) -> u16 {
        let at = self.0.binary_search_by_key(&action, |fired| fired.action);
        at.map_or(0, |at| self.0[at].shots)
    }

    /// Counts a shot of `action`, of `clip` at its rank: whether the shot emptied a clip that
    /// refills by time, which is then full again, so the weapon reloads. A clip that refills at
    /// home stays empty, its shots counted, until its home refills it (04-capabilities/combat.md, A
    /// clip may reload at home).
    pub(crate) fn shoot(&mut self, action: ActionId, clip: ClipRule) -> bool {
        let shots = self.fired(action) + 1;
        let reloads = shots >= clip.shots.get() && clip.refill == ClipRefill::Time;
        self.set(action, if reloads { 0 } else { shots });
        reloads
    }

    /// Whether the clip of `action`, of `shots`, is empty: it fired them all, as a clip that
    /// refills at home stays until its home refills it.
    pub fn is_empty(&self, action: ActionId, shots: NonZeroU16) -> bool {
        self.fired(action) >= shots.get()
    }

    /// Refills each clip of `slots` that refills at home, of `weapons`, by the share of `reload`
    /// that passed: the share of its shots back, rounded down, as Zero Hour fills a clip in
    /// proportion (`JetOrHeliReloadAmmoState::update`). The contract by which a unit's home
    /// refills its clips as its reload goes (04-capabilities/combat.md, A clip may reload at home).
    pub fn reload_at_home(
        &mut self,
        slots: &ActionSlots,
        weapons: &ById<ActionId, Weapon>,
        reload: HomeReload,
    ) {
        let HomeReload { passed, total } = reload;
        debug_assert!(passed <= total && total > Ticks::ZERO, "a reload's share");
        for (action, clip) in slots
            .iter()
            .filter_map(|slot| Self::home_clip(slot, weapons))
        {
            let fired = self.fired(action);
            let shots = NonZeroU64::from(clip.shots).get();
            let back = u16::try_from(shots * passed.get() / total.get())
                .expect("a share of a clip's shots fits");
            let left = fired.min(clip.shots.get() - back);
            if left != fired {
                self.set(action, left);
            }
        }
    }

    /// The clips of the weapons in `slots` that refill at home, each of `weapons`, at its slot's
    /// rank, its reload unsped, with the shots it fired: what a unit's home reloads
    /// (04-capabilities/combat.md, A clip may reload at home).
    pub fn at_home<'a>(
        &'a self,
        slots: &'a ActionSlots,
        weapons: &'a ById<ActionId, Weapon>,
    ) -> impl Iterator<Item = HomeClip> + 'a {
        slots.iter().filter_map(move |slot| {
            let (action, clip) = Self::home_clip(slot, weapons)?;
            Some(HomeClip {
                action,
                clip,
                fired: self.fired(action),
            })
        })
    }

    /// The weapon of `slot` and its clip at the slot's rank, of `weapons`, when the clip refills at
    /// home.
    fn home_clip(
        slot: ActionSlot,
        weapons: &ById<ActionId, Weapon>,
    ) -> Option<(ActionId, ClipRule)> {
        let action = slot.action?;
        let clip = weapons.get(action)?.clip(&[], slot.rank?)?;
        (clip.refill == ClipRefill::Home).then_some((action, clip))
    }

    /// Sets the shots `action` fired since its clip was full, removing a full clip's.
    fn set(&mut self, action: ActionId, shots: u16) {
        let at = self.0.binary_search_by_key(&action, |fired| fired.action);
        match (at, shots) {
            (Ok(at), 0) => {
                self.0.remove(at);
            }
            (Ok(at), _) => self.0[at].shots = shots,
            (Err(_), 0) => {}
            (Err(at), _) => self.0.insert(at, Fired { action, shots }),
        }
    }
}

/// A snapshot is untrusted, so weapons out of the order of their ids, one twice, or one of no shot,
/// which a full clip is not in, fail to decode.
impl<'de> Deserialize<'de> for ClipShots {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fired = Vec::<Fired>::deserialize(deserializer)?;
        if !fired.is_sorted_by(|a, b| a.action < b.action) || fired.iter().any(|f| f.shots == 0) {
            return Err(D::Error::custom(
                "weapons in order, each once, each of a shot",
            ));
        }
        Ok(Self(fired))
    }
}

impl SimComponent for ClipShots {
    const NAME: &'static str = "combat.clip_shots";
    type Host = UnitEntity;

    // A weapon the book lacks, or one whose clip holds no more shots than it fired at every rank,
    // has no clip to count; a clip that refills at home may stay empty.
    fn check(&self, world: &World, _: Entity) -> bool {
        let weapons = world.resource::<ById<ActionId, Weapon>>();
        self.0.iter().all(|fired| {
            let clips = weapons
                .get(fired.action)
                .and_then(|weapon| weapon.clip.as_deref());
            clips.is_some_and(|clips| {
                clips.iter().any(|clip| match clip.refill {
                    ClipRefill::Time => fired.shots < clip.shots.get(),
                    ClipRefill::Home => fired.shots <= clip.shots.get(),
                })
            })
        })
    }
}

impl Replication for ClipShots {
    const KIND: DataKind = DataKind::Cooldowns;
    type Sending = SentPredicted;
}

#[cfg(test)]
mod tests;
