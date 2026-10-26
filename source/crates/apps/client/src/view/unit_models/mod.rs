use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use bevy::animation::{AnimatedBy, AnimationClip, AnimationPlayer, AnimationTargetId};
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
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::ecs::world::EntityWorldMut;
use bevy::gltf::GltfMaterial;
use bevy::gltf::extensions::{
    ErasedGltfExtensionHandler, GltfExtensionHandler, GltfExtensionHandlers,
};
use bevy::gltf::gltf::{Animation, Material, Mesh, Primitive};
use bevy::math::Quat;
use bevy::mesh::Mesh3d;
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::transform::components::Transform;
use bevy::world_serialization::WorldInstanceReady;
use campfire_client_files::{AnimationExtras, ClientObject, MaterialExtras};
use campfire_combat::TurretAngle;
use campfire_common::DeclaredName;
use campfire_declarations::ById;
use campfire_script::MatchNames;
use campfire_sim::Unpredicted;
use campfire_units::{TypeOriginBook, Variations};
use campfire_values::{PackageIndex, Team, UnitType};
use serde::Deserialize;
use tracing::error;

use crate::view::client_data::ClientLoad;
use crate::view::drawing::{Drawing, DrawingOf};
use crate::view::file_material::{FileBlend, FileMaterial, FileMaterialPlugin};
use crate::view::float_num::FloatNum;
use crate::view::ground_heights::GroundHeights;
use crate::view::look_clips::{ClipHidden, ClipTiming, HiddenPivot};
use crate::view::match_colors::{MatchColors, Paint};
use crate::view::model_scenes::ModelScenes;
use crate::view::unit_looks::{LookModel, LookNodes, LookTypes, ModelSource, UnitLooks};

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
        FileMaterialPlugin::require(app);
        app.init_resource::<ModelScenes>();
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
                Self::redress,
                (Self::follow_teams, Self::repaint).chain(),
            ),
        );
    }
}

impl UnitModelsPlugin {
    /// Loads every unit type's models, once the match's types are known.
    fn load_looks(
        (origins, variations): (
            Res<'_, TypeOriginBook>,
            Option<Res<'_, ById<UnitType, Variations>>>,
        ),
        names: Res<'_, MatchNames>,
        load: Res<'_, ClientLoad>,
        (assets, mut scenes): (Res<'_, AssetServer>, ResMut<'_, ModelScenes>),
        mut commands: Commands<'_, '_>,
    ) {
        let types = LookTypes {
            origins: &origins,
            variations: variations.as_deref(),
            names: &names,
        };
        let looks = UnitLooks::of(types, &load);
        for model in looks.rest_models() {
            scenes.scene(&model.scene, &assets);
        }
        commands.insert_resource(looks);
    }

    /// Places each of the map's client objects (09-zero-hour.md, The multiplayer rule for client
    /// objects): a root on the ground at its point, at its height above the heightmap, turned by
    /// its angle as a placed unit is, and its look's models over it, each at its scale; a look the
    /// mode's package lacks is logged, and its object not drawn.
    fn place_objects(
        (load, heights): (Res<'_, ClientLoad>, Option<Res<'_, GroundHeights>>),
        (assets, mut scenes): (Res<'_, AssetServer>, ResMut<'_, ModelScenes>),
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
                    let Some(look) = looks.objects().get(&object.look) else {
                        let look = object.look.as_str();
                        error!(look, "a client object's look is none of its package's");
                        continue;
                    };
                    new.insert(LookModel::all(
                        ModelSource::still(PackageIndex::MODE),
                        &look.models,
                    ))
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
                let bundle = model.bundle(&mut scenes, &assets, Paint::default());
                commands.spawn((bundle, ChildOf(root)));
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
    /// paint; a model that replaces the one of its draw's state before is shown, and that one
    /// goes.
    fn dress(
        ready: On<'_, '_, WorldInstanceReady>,
        models: Query<'_, '_, (&LookNodes, &Paint, Option<&Replaces>)>,
        (children, names, meshes): (
            Query<'_, '_, &'static Children>,
            Query<'_, '_, &'static Name>,
            Query<'_, '_, (), With<Mesh3d>>,
        ),
        (parents, transforms): (Query<'_, '_, &ChildOf>, Query<'_, '_, &Transform>),
        colors: Res<'_, MatchColors>,
        mut commands: Commands<'_, '_>,
    ) {
        let Ok((look_nodes, &paint, replaces)) = models.get(ready.entity) else {
            return;
        };
        if let Some(&Replaces(before)) = replaces {
            commands.entity(before).despawn();
            commands
                .entity(ready.entity)
                .remove::<Replaces>()
                .insert(Visibility::Inherited);
        }
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
        for &node in &hidden {
            commands.entity(node).insert(Visibility::Hidden);
        }
        let tree = ModelTree {
            children: &children,
            names: &names,
            meshes: &meshes,
        };
        let pivots = tree.pivots(ready.entity);
        if let Some(pivots) = &pivots {
            commands
                .entity(pivots.player)
                .insert(AnimationPlayer::default());
            for (node, path) in tree.targets(pivots.player) {
                let target = AnimationTargetId::from_names(path.iter());
                commands
                    .entity(node)
                    .insert((target, AnimatedBy(pivots.player)));
            }
        }
        commands
            .entity(ready.entity)
            .remove::<Redress>()
            .insert(Dressed { hidden, pivots });
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

    /// Hides again the nodes of each dressed model whose state's hidden nodes changed, as a new
    /// state of the same model gives them: those it hid shown, and those its nodes now hide
    /// hidden.
    fn redress(
        mut models: Query<'_, '_, (Entity, &LookNodes, &mut Dressed), With<Redress>>,
        (children, names): (Query<'_, '_, &Children>, Query<'_, '_, &Name>),
        mut commands: Commands<'_, '_>,
    ) {
        for (model, look_nodes, mut dressed) in &mut models {
            for &node in &dressed.hidden {
                commands.entity(node).insert(Visibility::Inherited);
            }
            let spawned = children.iter_descendants(model).map(|node| Node {
                entity: node,
                name: names.get(node).ok().map(Name::as_str),
            });
            let Dressing { hidden, .. } = Dressing::of(look_nodes, spawned);
            for &node in &hidden {
                commands.entity(node).insert(Visibility::Hidden);
            }
            dressed.hidden = hidden;
            commands.entity(model).remove::<Redress>();
        }
    }
}

impl ModelTree<'_, '_, '_> {
    /// Whether `node` is a node of a mesh, a sub-object, as the glTF loader spawns each of its
    /// primitives a child of it.
    fn is_mesh(&self, node: Entity) -> bool {
        self.children
            .get(node)
            .is_ok_and(|children| children.iter().any(|&child| self.meshes.contains(child)))
    }

    /// The pivots of the model at `model`: the first node of its scene below its scene's root
    /// that is no mesh and holds others, the root pivot of its hierarchy, and each pivot below it
    /// of its own mesh nodes; none for a model of no such node, a mesh alone.
    fn pivots(&self, model: Entity) -> Option<ModelPivots> {
        let roots = self.children.get(model).into_iter().flatten();
        let tops = roots.flat_map(|&root| self.children.get(root).into_iter().flatten());
        let player = tops
            .copied()
            .find(|&top| !self.is_mesh(top) && self.children.get(top).is_ok())?;
        let mut meshes = Vec::new();
        for pivot in std::iter::once(player).chain(self.children.iter_descendants(player)) {
            if self.is_mesh(pivot) {
                continue;
            }
            let own: Vec<Entity> = self
                .children
                .get(pivot)
                .into_iter()
                .flatten()
                .copied()
                .filter(|&child| self.is_mesh(child))
                .collect();
            if let (false, Ok(name)) = (own.is_empty(), self.names.get(pivot)) {
                meshes.push(PivotMeshes {
                    pivot: name.as_str().to_owned(),
                    meshes: own,
                });
            }
        }
        Some(ModelPivots { player, meshes })
    }

    /// Each pivot of the hierarchy rooted at `root`, `root` itself first, with the names of the
    /// nodes from `root` down to it, as Bevy's glTF loader names a clip's target by them.
    fn targets(&self, root: Entity) -> Vec<(Entity, Vec<Name>)> {
        let mut targets = Vec::new();
        let mut below = vec![(root, Vec::new())];
        while let Some((node, mut path)) = below.pop() {
            let Ok(name) = self.names.get(node) else {
                continue;
            };
            path.push(name.clone());
            for &child in self.children.get(node).into_iter().flatten() {
                if !self.is_mesh(child) {
                    below.push((child, path.clone()));
                }
            }
            targets.push((node, path));
        }
        targets
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

    /// Adds the timing of an animation's clip, of its extension: its frames, their rate, and the
    /// frames each pivot is hidden in, by the name of the node its tracks move; an animation of
    /// no extension has none, and one whose extension does not read is logged.
    fn on_animation(
        &mut self,
        load_context: &mut LoadContext<'_>,
        gltf_animation: &Animation<'_>,
        _animation_clip: &mut AnimationClip,
    ) {
        let Some(value) = gltf_animation.extension_value(AnimationExtras::EXTENSION) else {
            return;
        };
        let extras = match AnimationExtras::deserialize(value) {
            Ok(extras) => extras,
            Err(problem) => {
                error!(error = %problem, "an animation's extras do not read");
                return;
            }
        };
        let node_name = |index: u32| {
            let channels = gltf_animation.channels();
            let mut nodes = channels.map(|channel| channel.target().node());
            let node =
                nodes.find(|node| u32::try_from(node.index()).is_ok_and(|at| at == index))?;
            node.name().map(str::to_owned)
        };
        let hidden = extras
            .hidden
            .into_iter()
            .filter_map(|frames| {
                Some(HiddenPivot {
                    pivot: node_name(frames.node)?,
                    runs: frames.runs,
                })
            })
            .collect();
        let timing = ClipTiming {
            frames: extras.frames,
            rate: extras.rate,
            hidden,
        };
        load_context.add_labeled_asset(
            format!("Animation{}/timing", gltf_animation.index()),
            timing,
        );
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

/// On a model spawned hidden for a new state: the model of the state before, which goes as this
/// one is dressed and shown.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Replaces(pub(crate) Entity);

/// On a kept model of a new state, whose hidden nodes changed, which its dressing hides again:
/// once it is dressed, or as it is dressed.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Redress;

/// On a dressed model: the nodes its dressing hid, and its pivots, none for a model of no
/// hierarchy.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
#[require(ClipHidden)]
pub(crate) struct Dressed {
    pub(crate) hidden: Vec<Entity>,
    pub(crate) pivots: Option<ModelPivots>,
}

/// A model's pivots, as its scene spawned them: the root of its hierarchy, which its clips' player
/// plays on, and each pivot's own mesh nodes, those of its sub-objects on it, by the pivot's name,
/// of each pivot of any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelPivots {
    pub(crate) player: Entity,
    pub(crate) meshes: Vec<PivotMeshes>,
}

/// A pivot's own mesh nodes, by its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PivotMeshes {
    pub(crate) pivot: String,
    pub(crate) meshes: Vec<Entity>,
}

/// The tree of a spawned model's nodes, as a dressing reads it.
#[derive(Debug, Clone, Copy)]
struct ModelTree<'q, 'w, 's> {
    children: &'q Query<'w, 's, &'static Children>,
    names: &'q Query<'w, 's, &'static Name>,
    meshes: &'q Query<'w, 's, (), With<Mesh3d>>,
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
