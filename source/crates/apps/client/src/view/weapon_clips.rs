use std::collections::BTreeMap;
use std::num::NonZeroU16;

use bevy::app::{App, Plugin, Startup};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Res};
use campfire_combat::Weapon;
use campfire_declarations::ById;
use campfire_values::{ActionId, Rank};

/// The shots of each clip a weapon fires in, by its action and rank, as combat's part of the
/// looks gives them as the client starts, so an attack's look names no book of combat's; none in
/// a mode of no combat (02-engine-core.md, A client module list; 02-engine-core.md, A model shows
/// its clip's rounds).
#[derive(Resource, Debug, Default)]
pub(crate) struct WeaponClips(BTreeMap<ActionId, Box<[NonZeroU16]>>);

impl WeaponClips {
    /// Whether the weapon of `action` fires in clips.
    pub(crate) fn has_clip(&self, action: ActionId) -> bool {
        self.0.contains_key(&action)
    }

    /// The shots of the clip of the weapon of `action` at `rank`, none for one of no clip.
    pub(crate) fn shots(&self, action: ActionId, rank: Rank) -> Option<NonZeroU16> {
        self.0.get(&action)?.get(rank.index()).copied()
    }

    /// The clips of each weapon of `weapons` that fires in clips.
    pub(crate) fn of(weapons: &ById<ActionId, Weapon>) -> Self {
        let clips = weapons.iter().filter_map(|(action, weapon)| {
            let rules = weapon.clip_rules()?;
            Some((action, rules.iter().map(|rule| rule.shots).collect()))
        });
        Self(clips.collect())
    }
}

/// Combat's part of the looks: the clips of its weapons.
#[derive(Debug)]
pub(crate) struct WeaponClipsPlugin;

impl Plugin for WeaponClipsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Self::read);
    }
}

impl WeaponClipsPlugin {
    /// Reads the clips of the match's weapons.
    fn read(weapons: Res<'_, ById<ActionId, Weapon>>, mut commands: Commands<'_, '_>) {
        commands.insert_resource(WeaponClips::of(&weapons));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clip_gives_its_shots_by_rank_and_no_clip_none() {
        let (rocket, gun) = (ActionId::new(0), ActionId::new(1));
        let shots = |shots: u16| NonZeroU16::new(shots).unwrap();
        let clips = WeaponClips(BTreeMap::from([(rocket, Box::from([shots(2), shots(4)]))]));
        assert!(clips.has_clip(rocket) && !clips.has_clip(gun));
        assert_eq!(clips.shots(rocket, Rank::FIRST), Some(shots(2)));
        assert_eq!(clips.shots(rocket, Rank::new(2).unwrap()), Some(shots(4)));
        assert_eq!(clips.shots(rocket, Rank::new(3).unwrap()), None);
        assert_eq!(clips.shots(gun, Rank::FIRST), None);
    }
}
