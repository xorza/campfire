use bevy_math::Vec3;
use campfire_client_files::{BarrelFlash, FireBarrel};

use crate::pose::RigidPose;
use crate::zero_hour::barrel_recoil::BarrelRecoil;
use crate::zero_hour::format::ini::object_ini::{HideShow, StateLook, WeaponBones};
use crate::zero_hour::{axes, game_time};

/// What a model's default look reads of it: its sub-objects, in the HLOD's order, each with its
/// full name, its pivot, whether the model has a node of it, and whether its mesh is hidden by
/// its flag; and its pivots' names, parents and poses. Names are lowercase.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ModelNodes {
    pub(crate) sub_objects: Vec<LookSubObject>,
    pub(crate) pivots: Vec<PivotName>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LookSubObject {
    pub(crate) name: String,
    pub(crate) pivot: usize,
    /// False for a collision box, which the game never draws.
    pub(crate) drawn: bool,
    pub(crate) hidden: bool,
}

/// The drawn sub-objects a list of shows and hides shows and hides, by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NodeChanges {
    pub(crate) show: Vec<String>,
    pub(crate) hide: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PivotName {
    pub(crate) name: String,
    pub(crate) parent: Option<usize>,
    /// Its pose in its parent's frame, on W3D's axes.
    pub(crate) pose: RigidPose,
}

/// Where a barrel's shot leaves its model at rest: the point of the model's frame where it starts,
/// and the way it leaves along, its launch bone's `x`, a unit, each on glTF's axes, `y` up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ModelBarrel {
    pub(crate) at: Vec3,
    pub(crate) way: Vec3,
}

impl ModelNodes {
    /// The names of the drawn sub-objects `look` leaves hidden, as `W3DModelDraw` sets a state:
    /// those hidden by their flag; then each hide or show in order, also over the sub-objects on
    /// the strict child pivots of its own; then each weapon slot's muzzle flashes, as
    /// `handleClientRecoil` hides each every frame its barrel's recoil does not start
    /// (09-zero-hour.md, A muzzle flash shows at its barrel's shot).
    pub(crate) fn hidden(&self, look: &StateLook) -> Vec<String> {
        let mut hidden: Vec<bool> = self
            .sub_objects
            .iter()
            .map(|sub_object| sub_object.hidden)
            .collect();
        for change in &look.hide_show {
            for at in self.touched(&change.name) {
                hidden[at] = change.hide;
            }
        }
        for at in self.flashes(look) {
            hidden[at] = true;
        }
        self.sub_objects
            .iter()
            .zip(hidden)
            .filter(|(sub_object, hidden)| sub_object.drawn && *hidden)
            .map(|(sub_object, _)| sub_object.name.clone())
            .collect()
    }

    /// The names of the drawn sub-objects `changes` show and hide, each as the last change that
    /// touches it says, as `W3DModelDraw::updateSubObjects` sets an upgrade's over a state's, but
    /// the muzzle flashes of `look`, which stay hidden.
    pub(crate) fn changes(&self, changes: &[HideShow], look: &StateLook) -> NodeChanges {
        let mut set: Vec<Option<bool>> = vec![None; self.sub_objects.len()];
        for change in changes {
            for at in self.touched(&change.name) {
                set[at] = Some(change.hide);
            }
        }
        for at in self.flashes(look) {
            set[at] = None;
        }
        let mut changed = NodeChanges::default();
        for (sub_object, hide) in self.sub_objects.iter().zip(set) {
            match hide.filter(|_| sub_object.drawn) {
                Some(true) => changed.hide.push(sub_object.name.clone()),
                Some(false) => changed.show.push(sub_object.name.clone()),
                None => {}
            }
        }
        changed
    }

    /// The places of the sub-objects a show or a hide of `name` touches, as `doHideShowSubObjs`
    /// does: its own, then those on the strict child pivots of its pivot; none for a name of no
    /// sub-object.
    fn touched(&self, name: &str) -> Vec<usize> {
        let Some(at) = self.sub_object(name) else {
            return Vec::new();
        };
        let pivot = self.sub_objects[at].pivot;
        let below = self.sub_objects.iter().enumerate();
        let below = below.filter(|(_, sub_object)| self.below(sub_object.pivot, pivot));
        std::iter::once(at)
            .chain(below.map(|(other, _)| other))
            .collect()
    }

    /// The places of the muzzle flashes of the weapon slots of `look`.
    fn flashes(&self, look: &StateLook) -> Vec<usize> {
        let slots = look.weapons.iter();
        let pivots =
            slots.flat_map(|weapon| self.barrel_pivots(weapon, |weapon| &weapon.muzzle_flash));
        pivots
            .flatten()
            .filter_map(|pivot| self.flash(pivot))
            .collect()
    }

    /// The place of the muzzle flash on `pivot`: its first sub-object, as `setMuzzleFlashHidden`
    /// takes it (`Get_Sub_Object_On_Bone(0, …)`); none for a pivot of none.
    fn flash(&self, pivot: usize) -> Option<usize> {
        self.sub_objects
            .iter()
            .position(|sub_object| sub_object.pivot == pivot)
    }

    /// A weapon slot's barrels, in the order its shots take them, of a draw module of `recoil`:
    /// each one's fire bone, its own, or for a barrel of a flash and of no fire bone of its number
    /// the barrel's before it, as `validateWeaponBarrelInfo` gives one fire bone to many flashes;
    /// and its drawn flash, shown for the frames `recoil` gives a barrel of a recoil bone or of
    /// none, of whole milliseconds as the engine counts frames, none for a recoil that does not
    /// end; none at all when no barrel has a fire bone or a flash.
    pub(crate) fn fire_barrels(
        &self,
        weapon: &WeaponBones,
        recoil: BarrelRecoil,
    ) -> Vec<FireBarrel> {
        let fires = self.barrel_pivots(weapon, |weapon| &weapon.fire_fx);
        let flashes = self.barrel_pivots(weapon, |weapon| &weapon.muzzle_flash);
        let recoils = self.barrel_pivots(weapon, |weapon| &weapon.recoil);
        let mut before = None;
        let mut barrels = Vec::with_capacity(fires.len());
        for ((fire, flash), recoils) in fires.into_iter().zip(flashes).zip(recoils) {
            let shared = flash.is_some() && weapon.fire_fx.is_some();
            let fire = fire.or_else(|| before.filter(|_| shared));
            before = fire;
            let flash = flash
                .and_then(|pivot| self.flash(pivot))
                .filter(|&at| self.sub_objects[at].drawn);
            let frames = recoil.flash_frames(recoils.is_some());
            let flash = flash.zip(frames).map(|(at, frames)| BarrelFlash {
                node: self.sub_objects[at].name.clone(),
                ms: u32::try_from(game_time::frames_ms(u64::from(frames)))
                    .expect("a minute of frames fits u32"),
            });
            barrels.push(FireBarrel {
                bone: fire.map(|pivot| self.pivots[pivot].name.clone()),
                flash,
            });
        }
        if barrels
            .iter()
            .all(|barrel| barrel.bone.is_none() && barrel.flash.is_none())
        {
            barrels.clear();
        }
        barrels
    }

    /// The pivot of `bone` of each of a weapon slot's barrels, as `validateWeaponBarrelInfo`
    /// finds them: for each number from 01 to 99, while any of the slot's bones has that number,
    /// a barrel, of its bone of that number, none for a barrel it lacks; else, if no number found
    /// one, one barrel of its bone's bare name.
    fn barrel_pivots(
        &self,
        weapon: &WeaponBones,
        bone: fn(&WeaponBones) -> &Option<String>,
    ) -> Vec<Option<usize>> {
        let bones = [
            &weapon.fire_fx,
            &weapon.recoil,
            &weapon.muzzle_flash,
            &weapon.launch,
        ];
        let mut barrels = Vec::new();
        for number in 1..=99 {
            let numbered =
                |bone: &Option<String>| self.pivot(&format!("{}{number:02}", bone.as_ref()?));
            if bones.iter().all(|bone| numbered(bone).is_none()) {
                break;
            }
            barrels.push(numbered(bone(weapon)));
        }
        if barrels.is_empty() {
            barrels.push(bone(weapon).as_ref().and_then(|bare| self.pivot(bare)));
        }
        barrels
    }

    /// The pose of each of its pivots in its model's frame, as `validateCachedBones` places its
    /// pristine bones: each pivot's pose in its parent's of `animated`, an animation's at one of
    /// its ends, when it holds one for each pivot, else its own, down its parents.
    pub(crate) fn pristine(&self, animated: Option<&[RigidPose]>) -> Vec<RigidPose> {
        let animated = animated.filter(|animated| animated.len() == self.pivots.len());
        let mut poses: Vec<RigidPose> = Vec::with_capacity(self.pivots.len());
        for (at, pivot) in self.pivots.iter().enumerate() {
            let local = animated.map_or(pivot.pose, |animated| animated[at]);
            let pose = match pivot.parent {
                Some(parent) => poses[parent].then(local),
                None => local,
            };
            poses.push(pose);
        }
        poses
    }

    /// The launches of a weapon slot's barrels, in the order its shots take them, of each
    /// barrel's launch bone of the poses `pristine`, or the model's place along its `x` for a
    /// barrel of none, as `validateWeaponBarrelInfo` keeps the identity for it; none when no
    /// barrel has one.
    pub(crate) fn launches(
        &self,
        weapon: &WeaponBones,
        pristine: &[RigidPose],
    ) -> Vec<ModelBarrel> {
        let barrels = self.barrel_pivots(weapon, |weapon| &weapon.launch);
        if barrels.iter().all(Option::is_none) {
            return Vec::new();
        }
        let placed = |pose: RigidPose| ModelBarrel {
            at: axes::y_up(pose.translation.to_array()).into(),
            way: axes::y_up(pose.rotation.rotate(Vec3::X).to_array()).into(),
        };
        barrels
            .into_iter()
            .map(|pivot| placed(pivot.map_or(RigidPose::IDENTITY, |pivot| pristine[pivot])))
            .collect()
    }

    /// The point of its pivot `name`, of either case, of the poses `pristine`, on glTF's axes;
    /// none for a name of no pivot.
    pub(crate) fn pivot_point(&self, name: &str, pristine: &[RigidPose]) -> Option<Vec3> {
        let pose = pristine[self.pivot(name)?];
        Some(axes::y_up(pose.translation.to_array()).into())
    }

    /// The drawn sub-objects that show each round of a weapon slot's clip, in the order its clip
    /// fires them, as `doHideShowProjectileObjects` hides them: those its `WeaponHideShowBone`
    /// touches, one round for the whole clip; else, for each number from 01, those of its launch
    /// bone's name with that number, up to the last number the model holds, none for a number it
    /// lacks. A show or a hide of a name touches its sub-object and those on its pivot's children,
    /// but the muzzle flashes of `look`, which stay hidden.
    pub(crate) fn rounds(&self, weapon: &WeaponBones, look: &StateLook) -> Vec<Vec<String>> {
        let flashes = self.flashes(look);
        let drawn = |name: &str| -> Vec<String> {
            let touched = self.touched(name).into_iter();
            let touched = touched.filter(|at| self.sub_objects[*at].drawn && !flashes.contains(at));
            touched
                .map(|at| self.sub_objects[at].name.clone())
                .collect()
        };
        if let Some(name) = &weapon.rounds {
            let round = drawn(name);
            return if round.is_empty() {
                Vec::new()
            } else {
                vec![round]
            };
        }
        let Some(launch) = &weapon.launch else {
            return Vec::new();
        };
        let mut rounds: Vec<Vec<String>> = (1..=99)
            .map(|number| drawn(&format!("{launch}{number:02}")))
            .collect();
        let held = rounds.iter().rposition(|round| !round.is_empty());
        rounds.truncate(held.map_or(0, |last| last + 1));
        rounds
    }

    /// The name of its pivot `name`, of either case, as its node is named.
    pub(crate) fn pivot_name(&self, name: &str) -> Option<&str> {
        Some(self.pivots[self.pivot(name)?].name.as_str())
    }

    fn pivot(&self, name: &str) -> Option<usize> {
        self.pivots
            .iter()
            .position(|pivot| pivot.name.eq_ignore_ascii_case(name))
    }

    /// The sub-object of `name`, as `Get_Sub_Object_By_Name` finds it: by its full name, else by
    /// the part of it after its first `.`, ignoring case.
    fn sub_object(&self, name: &str) -> Option<usize> {
        let nodes = &self.sub_objects;
        nodes
            .iter()
            .position(|sub_object| sub_object.name.eq_ignore_ascii_case(name))
            .or_else(|| {
                nodes.iter().position(|sub_object| {
                    let suffix = sub_object
                        .name
                        .split_once('.')
                        .map_or(sub_object.name.as_str(), |(_, suffix)| suffix);
                    suffix.eq_ignore_ascii_case(name)
                })
            })
    }

    /// Whether `pivot` lies strictly below `ancestor`; pivot 0, the root, lies below none.
    fn below(&self, pivot: usize, ancestor: usize) -> bool {
        let mut at = pivot;
        while at != 0 {
            match self.pivots.get(at).and_then(|pivot| pivot.parent) {
                Some(parent) => at = parent,
                None => return false,
            }
            if at == ancestor {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests;
