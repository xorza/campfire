use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::asset::{AssetServer, Handle, LoadContext};
use bevy::camera::visibility::Visibility;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::name::Name;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Added, Allow, Changed, With};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res};
use bevy::ecs::world::EntityWorldMut;
use bevy::gltf::GltfMaterial;
use bevy::gltf::extensions::{
    ErasedGltfExtensionHandler, GltfExtensionHandler, GltfExtensionHandlers,
};
use bevy::gltf::gltf::{Material, Mesh, Primitive};
use bevy::math::Quat;
use bevy::mesh::Mesh3d;
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::transform::components::Transform;
use bevy::world_serialization::WorldInstanceReady;
use campfire_client_files::{ClientObject, MaterialExtras};
use campfire_combat::TurretAngle;
use campfire_common::DeclaredName;
use campfire_sim::Unpredicted;
use campfire_units::TypeOriginBook;
use campfire_values::{PackageIndex, Team};
use serde::Deserialize;
use tracing::error;

use crate::view::client_data::ClientLoad;
use crate::view::drawing::{Drawing, DrawingOf};
use crate::view::file_material::{FileBlend, FileMaterial, FileMaterialPlugin};
use crate::view::float_num::FloatNum;
use crate::view::ground_heights::GroundHeights;
use crate::view::match_colors::{MatchColors, Paint};
use crate::view::unit_looks::{LookModel, LookNodes, UnitLooks};

/// Draws units and the map's client objects with their packages' models: loads each unit type's
/// models, each primitive in the file material of its glTF material and its extras, once for each
/// model as it loads, places each client object still, and dresses each model once its scene
/// spawns: the nodes its look hides hidden, the node its turret angle turns found, as a look's
/// hidden nodes are its own, and each mesh tagged with its paint, which it paints again as its
/// paint changes (02-engine-core.md, Material extras in a glTF extension;
/// 02-engine-core.md, Team colors in the one material).
#[derive(Debug)]
pub(crate) struct UnitModelsPlugin;

/// The root of a client object's drawing, which stands still where the map places it, of no unit.
#[derive(Component, Debug)]
pub(crate) struct StillObject;

/// The glTF loader's part that gives each primitive the file material of its glTF material, as a
/// model loads.
#[derive(Debug, Default, Clone)]
struct ExtrasHandler;

impl Plugin for UnitModelsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FileMaterialPlugin);
        // The glTF loader takes its handlers as it is finished, after every plugin's build.
        app.world_mut()
            .resource_mut::<GltfExtensionHandlers>()
            .0
            .write_blocking()
            .push(Box::new(ExtrasHandler));
        app.add_systems(
            Startup,
            (MatchColors::upload, (Self::load_looks, Self::place_objects)).chain(),
        );
        app.add_observer(Self::dress);
        app.add_systems(
            Update,
            (
                Self::turn_turrets,
                (Self::follow_teams, Self::repaint).chain(),
            ),
        );
    }
}

impl UnitModelsPlugin {
    /// Loads every unit type's models, once the match's types are known.
    fn load_looks(
        origins: Res<'_, TypeOriginBook>,
        load: Res<'_, ClientLoad>,
        assets: Res<'_, AssetServer>,
        mut commands: Commands<'_, '_>,
    ) {
        commands.insert_resource(UnitLooks::of(&origins, &load, &assets));
    }

    /// Places each of the map's client objects (09-zero-hour.md, The multiplayer rule for client
    /// objects): a root on the ground at its point, at its height above the heightmap, turned by
    /// its angle as a placed unit is, and its look's models over it, each at its scale; a look the
    /// mode's package lacks is logged, and its object not drawn.
    fn place_objects(
        (load, heights): (Res<'_, ClientLoad>, Option<Res<'_, GroundHeights>>),
        assets: Res<'_, AssetServer>,
        mut commands: Commands<'_, '_>,
    ) {
        let Some(looks) = load.packages.first().map(|package| &package.units) else {
            return;
        };
        let heights = heights.as_deref();
        // Each look's models resolve once, for every object of it.
        let mut resolved: BTreeMap<&DeclaredName, Vec<LookModel>> = BTreeMap::new();
        for object in &load.objects.objects {
            let models = match resolved.entry(&object.look) {
                Entry::Occupied(models) => models.into_mut(),
                Entry::Vacant(new) => {
                    let Some(look) = looks.objects.get(&object.look) else {
                        let look = object.look.as_str();
                        error!(look, "a client object's look is none of its package's");
                        continue;
                    };
                    new.insert(LookModel::all(PackageIndex::MODE, look, &assets))
                }
            };
            let root = commands
                .spawn((
                    StillObject,
                    Self::placed(object, heights),
                    Visibility::default(),
                ))
                .id();
            for model in &*models {
                commands.spawn((model.bundle(Paint::default()), ChildOf(root)));
            }
        }
    }

    /// Where `object` stands: its ground point, its height above the ground `heights` gives
    /// there, 0 on a map with none, and turned by its angle about the vertical.
    fn placed(object: &ClientObject, heights: Option<&GroundHeights>) -> Transform {
        let [x, z] = object.position.map(FloatNum::float);
        let beneath = heights.map_or(0.0, |heights| heights.at(x, z));
        let y = beneath + object.height.float();
        let turn = Quat::from_rotation_y(object.angle.float().to_radians());
        Transform::from_xyz(x, y, z).with_rotation(turn)
    }

    /// Turns the turret node of each drawing by its unit's turret angle, as the sim last gave it:
    /// of each unit whose angle changed, and of each drawing whose model was just dressed, so a
    /// node is written only when its angle or its node is new.
    fn turn_turrets(
        changed: TurnedTurrets<'_, '_>,
        dressed: Query<'_, '_, (&DrawingOf, &TurretNode), Added<TurretNode>>,
        (angles, turrets): (
            Query<'_, '_, &TurretAngle, Allow<Unpredicted>>,
            Query<'_, '_, &TurretNode>,
        ),
        mut nodes: Query<'_, '_, &mut Transform>,
    ) {
        let changed = changed.iter().filter_map(|(&angle, drawing)| {
            turrets
                .get(drawing.root())
                .ok()
                .map(|&turret| (angle, turret))
        });
        let dressed = dressed.iter().filter_map(|(&DrawingOf(unit), &turret)| {
            angles.get(unit).ok().map(|&angle| (angle, turret))
        });
        for (angle, turret) in changed.chain(dressed) {
            if let Ok(mut node) = nodes.get_mut(turret.node) {
                node.rotation = turret.rotation(angle);
            }
        }
    }

    /// Paints the models of each unit whose team changed, as a change of owner moves it to
    /// another team.
    fn follow_teams(
        units: Recolored<'_, '_>,
        roots: Query<'_, '_, &Children>,
        mut paints: Query<'_, '_, &mut Paint>,
    ) {
        for (&team, drawing) in &units {
            let Ok(models) = roots.get(drawing.root()) else {
                continue;
            };
            let mut models = paints.iter_many_mut(models);
            while let Some(paint) = models.fetch_next() {
                if let Ok(paint) = paint {
                    paint
                        .map_unchanged(|paint| &mut paint.team)
                        .set_if_neq(Some(team));
                }
            }
        }
    }

    /// Tags each mesh of each model whose paint changed, once its scene spawned; a model whose
    /// scene spawns after is tagged as it is dressed.
    fn repaint(
        models: Query<'_, '_, (Entity, &Paint), Changed<Paint>>,
        (children, meshes): (Query<'_, '_, &Children>, Query<'_, '_, (), With<Mesh3d>>),
        colors: Res<'_, MatchColors>,
        mut commands: Commands<'_, '_>,
    ) {
        for (model, &paint) in &models {
            let tag = colors.tag(paint);
            for node in children.iter_descendants(model) {
                if meshes.contains(node) {
                    commands.entity(node).insert(tag.clone());
                }
            }
        }
    }

    /// Dresses the model whose scene spawned, as its dressing says, and tags its meshes with its
    /// paint.
    fn dress(
        ready: On<'_, '_, WorldInstanceReady>,
        models: Query<'_, '_, (&LookNodes, &Paint)>,
        (children, names, meshes): (
            Query<'_, '_, &Children>,
            Query<'_, '_, &Name>,
            Query<'_, '_, (), With<Mesh3d>>,
        ),
        (parents, transforms): (Query<'_, '_, &ChildOf>, Query<'_, '_, &Transform>),
        colors: Res<'_, MatchColors>,
        mut commands: Commands<'_, '_>,
    ) {
        let Ok((look_nodes, &paint)) = models.get(ready.entity) else {
            return;
        };
        let tag = colors.tag(paint);
        for node in children.iter_descendants(ready.entity) {
            if meshes.contains(node) {
                commands.entity(node).insert(tag.clone());
            }
        }
        let spawned = children.iter_descendants(ready.entity).map(|node| Node {
            entity: node,
            name: names.get(node).ok().map(Name::as_str),
        });
        let Dressing { hidden, turret } = Dressing::of(look_nodes, spawned);
        for node in hidden {
            commands.entity(node).insert(Visibility::Hidden);
        }
        // A model is a child of its unit's drawing, which the turret's node turns with.
        if let Some(node) = turret
            && let (Ok(root), Ok(rest)) = (parents.get(ready.entity), transforms.get(node))
        {
            commands.entity(root.parent()).insert(TurretNode {
                node,
                rest: rest.rotation,
            });
        }
    }
}

impl ExtrasHandler {
    /// The extras of `material`, which its extension holds; none for a material of no extras, and
    /// none, logged, for extras that do not read.
    fn extras_of(material: &Material<'_>) -> Option<MaterialExtras> {
        let value = material.extension_value(MaterialExtras::EXTENSION)?;
        MaterialExtras::deserialize(value)
            .map_err(|problem| error!(error = %problem, "a material's extras do not read"))
            .ok()
    }

    /// The label of the file material of the glTF material of `label`.
    fn file_label(label: &str) -> String {
        format!("{label}/file")
    }
}

impl GltfExtensionHandler for ExtrasHandler {
    fn dyn_clone(&self) -> Box<dyn ErasedGltfExtensionHandler> {
        Box::new(Self)
    }

    /// Adds the file material of a glTF material, of the material as Bevy reads it and its
    /// extras, or the extras of its alpha mode for a material of none.
    fn on_material(
        &mut self,
        load_context: &mut LoadContext<'_>,
        gltf_material: &Material<'_>,
        _material: Handle<GltfMaterial>,
        material_asset: &GltfMaterial,
        material_label: &str,
    ) {
        let extras =
            Self::extras_of(gltf_material).unwrap_or_else(|| FileBlend::plain(material_asset));
        let file = FileBlend::material(material_asset, extras);
        load_context.add_labeled_asset(Self::file_label(material_label), file);
    }

    /// Draws a primitive by its file material, in place of the standard material Bevy's own part
    /// gives it.
    fn on_spawn_mesh_and_material(
        &mut self,
        load_context: &mut LoadContext<'_>,
        _primitive: &Primitive<'_>,
        _mesh: &Mesh<'_>,
        _material: &Material<'_>,
        entity: &mut EntityWorldMut<'_>,
        material_label: &str,
    ) {
        let file: Handle<FileMaterial> =
            load_context.get_label_handle(Self::file_label(material_label));
        entity
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(file));
    }
}

/// The units whose team changed, each with its drawing.
type Recolored<'w, 's> =
    Query<'w, 's, (&'static Team, &'static Drawing), (Changed<Team>, Allow<Unpredicted>)>;

/// The units whose turret angle changed, the client's own and those it only receives, each with
/// its drawing.
type TurnedTurrets<'w, 's> = Query<
    'w,
    's,
    (&'static TurretAngle, &'static Drawing),
    (Changed<TurretAngle>, Allow<Unpredicted>),
>;

/// A node of a spawned model: its entity, and its name.
#[derive(Debug, Clone, Copy)]
struct Node<'a> {
    entity: Entity,
    name: Option<&'a str>,
}

/// What a spawned model's dressing does: the nodes it hides, and the node its unit's turret angle
/// turns.
#[derive(Debug, Default, PartialEq, Eq)]
struct Dressing {
    hidden: Vec<Entity>,
    turret: Option<Entity>,
}

/// On a unit's drawing: the node of its model its turret angle turns, and that node's rotation
/// at rest, as its model holds it.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct TurretNode {
    node: Entity,
    rest: Quat,
}

impl TurretNode {
    /// The node's rotation at `angle`: its rest turned about the model's up by the angle, as the
    /// drawing turns by its unit's facing.
    fn rotation(&self, angle: TurretAngle) -> Quat {
        Quat::from_rotation_y(angle.degrees().float().to_radians()) * self.rest
    }
}

impl Dressing {
    /// The dressing of a model of `look_nodes`, whose scene spawned `nodes`: each node of a name it
    /// hides, and the first node of its turret's name.
    fn of<'a>(look_nodes: &LookNodes, nodes: impl Iterator<Item = Node<'a>>) -> Self {
        let mut dressing = Self::default();
        for node in nodes {
            if node
                .name
                .is_some_and(|name| look_nodes.hide.iter().any(|hidden| hidden == name))
            {
                dressing.hidden.push(node.entity);
            }
            let turret = look_nodes.turret.as_deref();
            if dressing.turret.is_none() && node.name.is_some_and(|name| Some(name) == turret) {
                dressing.turret = Some(node.entity);
            }
        }
        dressing
    }
}

#[cfg(test)]
mod tests;
