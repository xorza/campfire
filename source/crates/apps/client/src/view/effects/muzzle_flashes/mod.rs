use std::time::Duration;

use bevy::camera::visibility::Visibility;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::name::Name;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::time::Time;
use campfire_client_files::BarrelFlash;

/// The muzzle flashes shots show, each node until the time it hides again, as Zero Hour shows a
/// barrel's flash while its recoil starts and hides it each frame after (`W3DModelDraw::
/// handleClientRecoil`); and a scratch of the nodes a walk of a drawing has yet to visit
/// (02-engine-core.md, A shot shows its barrel's flash).
#[derive(Resource, Debug, Default)]
pub(crate) struct MuzzleFlashes {
    shown: Vec<ShownFlash>,
    open: Vec<Entity>,
}

/// A node a shot shows, and the time it hides again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShownFlash {
    node: Entity,
    until: Duration,
}

/// What a flash shows through: the drawings' nodes, their names and their visibilities, and the
/// frame's time.
#[derive(Debug)]
pub(crate) struct FlashParts<'a, 'w, 's> {
    pub(crate) children: &'a Query<'w, 's, &'static Children>,
    pub(crate) names: &'a Query<'w, 's, &'static Name>,
    pub(crate) visibilities: &'a mut Query<'w, 's, &'static mut Visibility>,
    pub(crate) now: Duration,
}

impl MuzzleFlashes {
    /// Shows each node of the name of `flash` below the drawing at `root`, which its state hides,
    /// for its time from now; a node shown already, from now again, as a shot starts its barrel's
    /// recoil again.
    pub(crate) fn show(
        &mut self,
        parts: &mut FlashParts<'_, '_, '_>,
        root: Entity,
        flash: &BarrelFlash,
    ) {
        let until = parts.now + Duration::from_millis(u64::from(flash.ms));
        self.open.clear();
        self.open.push(root);
        while let Some(at) = self.open.pop() {
            if parts
                .names
                .get(at)
                .is_ok_and(|name| name.as_str() == flash.node)
                && let Ok(mut visibility) = parts.visibilities.get_mut(at)
            {
                visibility.set_if_neq(Visibility::Inherited);
                match self.shown.iter_mut().find(|shown| shown.node == at) {
                    Some(shown) => shown.until = until,
                    None => self.shown.push(ShownFlash { node: at, until }),
                }
            }
            if let Ok(below) = parts.children.get(at) {
                self.open.extend(below.iter());
            }
        }
    }

    /// Hides each flash whose time ended, of a node still drawn.
    pub(crate) fn hide_ended(
        time: Res<'_, Time>,
        mut flashes: ResMut<'_, Self>,
        mut visibilities: Query<'_, '_, &mut Visibility>,
    ) {
        if flashes.shown.is_empty() {
            return;
        }
        let now = time.elapsed();
        flashes.shown.retain(|shown| {
            if now < shown.until {
                return true;
            }
            if let Ok(mut visibility) = visibilities.get_mut(shown.node) {
                visibility.set_if_neq(Visibility::Hidden);
            }
            false
        });
    }
}

#[cfg(test)]
mod tests;
