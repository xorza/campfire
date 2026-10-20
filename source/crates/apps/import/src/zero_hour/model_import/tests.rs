use campfire_client_files::{Blend, MaterialExtras, TeamPaint};
use campfire_codec::Json;
use campfire_store::internals::TestDir;

use gltf_json::material::AlphaMode;
use gltf_json::mesh::{Primitive, Semantic};
use gltf_json::texture::WrappingMode;
use gltf_json::validation::Checked;
use gltf_json::{Accessor, Index, Material, Root};

use super::*;
use crate::texture::srgb;
use crate::zero_hour::format::w3d::hierarchy::Pivot;
use crate::zero_hour::format::w3d::w3d_mesh::samples::GRADIENT_MODULATE;
use crate::zero_hour::format::w3d::w3d_mesh::{Ids, Pass, Shader, Stage, Texture, VertexMaterial};
use crate::zero_hour::install::Install;
use crate::zero_hour::install::samples::fixture;
use crate::zero_hour::model_nodes::LookSubObject;

fn convert(name: &str, assets: &mut ModelAssets<'_>) -> ModelImport {
    match ModelImport::convert(name, assets).unwrap() {
        Converted::Model(model) => *model,
        other => panic!("{name} converts to {other:?}"),
    }
}

/// The accessor of `semantic` that `primitive` draws with.
fn attribute(primitive: &Primitive, semantic: Semantic) -> Option<Index<Accessor>> {
    primitive.attributes.get(&Checked::Valid(semantic)).copied()
}

/// The places `indices` name.
fn places<'a, T: 'a>(indices: impl IntoIterator<Item = &'a Index<T>>) -> Vec<usize> {
    indices.into_iter().map(Index::value).collect()
}

/// `material`'s extras, which its own extension holds.
fn extras(material: &Material) -> Option<MaterialExtras> {
    let extensions = material.extensions.as_ref()?;
    let value = extensions.others.get(MaterialExtras::EXTENSION)?;
    Some(Json::convert(value).unwrap())
}

/// Whether `material` is unlit.
fn is_unlit(material: &Material) -> bool {
    let extensions = material.extensions.as_ref();
    extensions.is_some_and(|extensions| extensions.unlit.is_some())
}

/// Each material's name, base color, texture, alpha mode, two-sidedness and emissive color.
type MaterialRow<'a> = (&'a str, [f32; 4], Option<usize>, AlphaMode, bool, [f32; 3]);

/// Each node's name, children, translation, rotation and mesh.
type NodeRow<'a> = (
    &'a str,
    Vec<usize>,
    Option<[f32; 3]>,
    Option<[f32; 4]>,
    Option<usize>,
);

/// The fixture's tank, converted.
fn tank() -> ModelImport {
    let test_dir = TestDir::new();
    fixture(&test_dir);
    let install = Install::open(&test_dir.path("zh")).unwrap();
    let mut assets = ModelAssets::new(&install).unwrap();
    convert("Tank", &mut assets)
}

#[test]
fn an_hlod_becomes_nodes_on_its_pivots_with_meshes_on_glb_axes() {
    let tank = tank();
    let document = &tank.glb.document;

    // The pivots, each `(x, y, z)` as `(x, z, −y)`: the turret at (1, 3, −2), its half turn
    // about W3D's `z` one about glTF's `y`; the flash bone at (0, 0, −1). Then a node for
    // each sub-object that is a mesh, on its pivot's node; the skin's at the root. The box has
    // none, and the name of no render object is dropped.
    let nodes: Vec<NodeRow<'_>> = document
        .nodes
        .iter()
        .map(|node| {
            (
                node.name.as_deref().unwrap(),
                places(node.children.iter().flatten()),
                node.translation,
                node.rotation.map(|rotation| rotation.0),
                node.mesh.map(|mesh| mesh.value()),
            )
        })
        .collect();
    let identity = Some([0.0, 0.0, 0.0, 1.0]);
    assert_eq!(
        nodes,
        [
            ("roottransform", vec![1, 3], Some([0.0; 3]), identity, None),
            (
                "turret",
                vec![2, 4],
                Some([1.0, 3.0, -2.0]),
                Some([0.0, 1.0, 0.0, 0.0]),
                None
            ),
            (
                "muzzlefx01",
                vec![5],
                Some([0.0, 0.0, -1.0]),
                identity,
                None
            ),
            ("tank.hull", vec![], None, None, Some(0)),
            ("tank.turret", vec![], None, None, Some(1)),
            ("tank.flash", vec![], None, None, Some(2)),
            ("tank.skin", vec![], None, None, Some(3)),
        ]
    );
    assert_eq!(places(&document.scenes[0].nodes), [0, 6]);
    let names: Vec<&str> = document
        .meshes
        .iter()
        .map(|mesh| mesh.name.as_deref().unwrap())
        .collect();
    assert_eq!(
        names,
        ["tank.hull", "tank.turret", "tank.flash", "tank.skin"]
    );

    // The hull: its vertices on glTF's axes, `V` as `1 − V`; its untextured triangle first,
    // as no texture sorts before one, then its textured one.
    let glb = &tank.glb;
    let hull = &document.meshes[0].primitives;
    let hull_of = |semantic| attribute(&hull[0], semantic).unwrap();
    assert_eq!(
        glb.floats_of(hull_of(Semantic::Positions)),
        [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 1.0, 0.0, -1.0]
    );
    assert_eq!(
        glb.floats_of(hull_of(Semantic::Normals)),
        [0.0, 1.0, 0.0].repeat(4)
    );
    // The last vertex's `NaN` and infinite coordinates are 0.
    assert_eq!(
        glb.floats_of(hull_of(Semantic::TexCoords(0))),
        [0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(
        glb.colors_of(hull_of(Semantic::Colors(0))),
        [10, 20, 30, 40].repeat(4)
    );
    assert_eq!(hull.len(), 2);
    assert_eq!(glb.indices_of(hull[0].indices.unwrap()), [2, 1, 3]);
    assert_eq!(glb.indices_of(hull[1].indices.unwrap()), [0, 1, 2]);
    assert_eq!(
        places(hull.iter().flat_map(|primitive| &primitive.material)),
        [0, 1]
    );

    // The turret's zero normals: the first vertex's from its triangle's corners (0, 0, 0),
    // (1, 0, 0) and (0, 1, 0), whose normal is (0, 0, 1); the vertex no triangle uses, up.
    // Its other normals, (0, −1, 0), are (0, 0, 1) on glTF's axes.
    let turret = &document.meshes[1].primitives[0];
    assert_eq!(
        glb.floats_of(attribute(turret, Semantic::Normals).unwrap()),
        [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0]
    );
    assert_eq!(attribute(turret, Semantic::TexCoords(0)), None);

    // The flash's stage of no coordinates samples D3D's (0, 0), which is glTF's, at each
    // vertex; its texture is in no archive, so its triangle is untextured.
    let flash = &document.meshes[2].primitives[0];
    let flash_coordinates = attribute(flash, Semantic::TexCoords(0)).unwrap();
    assert_eq!(glb.floats_of(flash_coordinates), [0.0; 6]);
    assert_eq!(attribute(flash, Semantic::Colors(0)), None);

    // The skin's vertices placed by the turret's pose into the model's frame: the half turn
    // takes (x, y, z) to (−x, −y, z), then the move by (1, 2, 3): (1, 0, 0) to (0, 2, 3),
    // (0, 1, 0) to (1, 1, 3), (0, 0, 1) to (1, 2, 4); on glTF's axes (0, 3, −2), (1, 3, −1)
    // and (1, 4, −2). Its normals turn with it, (0, 0, 1) staying up.
    let skin = &document.meshes[3].primitives[0];
    assert_eq!(
        glb.floats_of(attribute(skin, Semantic::Positions).unwrap()),
        [0.0, 3.0, -2.0, 1.0, 3.0, -1.0, 1.0, 4.0, -2.0]
    );
    assert_eq!(
        glb.floats_of(attribute(skin, Semantic::Normals).unwrap()),
        [0.0, 1.0, 0.0].repeat(3)
    );
    // The turret and the skin share one material: of no texture, under the default shader,
    // which textures.
    let materials = [turret, skin, flash].map(|primitive| primitive.material.unwrap().value());
    assert_eq!(materials, [2, 2, 3]);
}

#[test]
fn an_hlod_s_materials_are_its_shaders_and_its_parts_what_its_look_reads() {
    let tank = tank();
    let document = &tank.glb.document;
    // The materials, each of the model's name and its index: the hull's diffuse (255, 128,
    // 0) as linear light; its texture `Rock.tga` is the archives' `Rock.dds`, clamped in
    // `u`; the flash adds, which glTF's material cannot say, so it has its extras, and
    // its texture `Gone.tga` is in no archive, so its white is `MissingTexture`'s
    // `0x7FFF00FF`: red and blue 1, green 0, alpha 127 / 255.
    let half = f32::from(u16::try_from(srgb::light(128)).unwrap()) / 65_536.0;
    let rows: Vec<MaterialRow<'_>> = document
        .materials
        .iter()
        .map(|material| {
            let pbr = &material.pbr_metallic_roughness;
            let texture = pbr.base_color_texture.as_ref();
            (
                material.name.as_deref().unwrap(),
                pbr.base_color_factor.0,
                texture.map(|texture| texture.index.value()),
                *material.alpha_mode.as_ref().unwrap(),
                material.double_sided,
                material.emissive_factor.0,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("tank_0", [0.0; 4], None, AlphaMode::Opaque, false, [0.0; 3]),
            (
                "tank_1",
                [1.0, half, 0.0, 1.0],
                Some(0),
                AlphaMode::Opaque,
                false,
                [0.0; 3]
            ),
            ("tank_2", [0.0; 4], None, AlphaMode::Opaque, false, [0.0; 3]),
            (
                "tank_3",
                [1.0, 0.0, 1.0, 127.0 / 255.0],
                None,
                AlphaMode::Blend,
                true,
                [1.0, 0.0, 0.0]
            ),
        ]
    );
    // Each is rough, not metallic and of no specular, and lit, as each shader modulates; the
    // flash alone has extras, of its additive blend and no depth write, which its extension
    // names.
    for material in &document.materials {
        let pbr = &material.pbr_metallic_roughness;
        let extensions = material.extensions.as_ref().unwrap();
        assert_eq!((pbr.metallic_factor.0, pbr.roughness_factor.0), (0.0, 1.0));
        assert_eq!(extensions.specular.as_ref().unwrap().specular_factor.0, 0.0);
        assert!(!is_unlit(material));
    }
    let extras: Vec<Option<MaterialExtras>> = document.materials.iter().map(extras).collect();
    let flash = MaterialExtras {
        blend: Blend::Add,
        depth_write: false,
        depth_bias: 0,
        team: TeamPaint::NONE,
    };
    assert_eq!(extras, [None, None, None, Some(flash)]);
    assert_eq!(
        document.extensions_used,
        ["CAMPFIRE_material_extras", "KHR_materials_specular"]
    );
    assert_eq!(
        document.images[0].uri.as_deref(),
        Some("../textures/art/textures/rock.ktx2")
    );
    let sampler = &document.samplers[0];
    assert_eq!(
        (
            sampler.wrap_s.as_ref().unwrap(),
            sampler.wrap_t.as_ref().unwrap()
        ),
        (&WrappingMode::ClampToEdge, &WrappingMode::Repeat)
    );
    // Its aggregate, which no glTF node draws, is reported by its name.
    assert_eq!(tank.aggregates, ["tank.smoke"]);
    assert_eq!(
        tank.missing_textures,
        BTreeSet::from(["gone.tga".to_owned()])
    );

    // What the default look reads: each sub-object the HLOD keeps, the box with no node.
    let sub_object = |name: &str, pivot, drawn, hidden| LookSubObject {
        name: name.to_owned(),
        pivot,
        drawn,
        hidden,
    };
    assert_eq!(
        tank.nodes.sub_objects,
        [
            sub_object("tank.hull", 0, true, false),
            sub_object("tank.turret", 1, true, false),
            sub_object("tank.flash", 2, true, false),
            sub_object("tank.pickbox", 0, false, true),
            sub_object("tank.skin", 0, true, false),
        ]
    );
    let pivots: Vec<(&str, Option<usize>)> = tank
        .nodes
        .pivots
        .iter()
        .map(|pivot| (pivot.name.as_str(), pivot.parent))
        .collect();
    assert_eq!(
        pivots,
        [
            ("roottransform", None),
            ("turret", Some(0)),
            ("muzzlefx01", Some(1))
        ]
    );
}

#[test]
fn a_name_is_an_hlod_a_mesh_alone_or_a_render_object_that_draws_nothing() {
    let test_dir = TestDir::new();
    fixture(&test_dir);
    let install = Install::open(&test_dir.path("zh")).unwrap();
    let mut assets = ModelAssets::new(&install).unwrap();
    // `File.Mesh` is a mesh alone, at the scene's root, and its materials are named by it.
    let hull = convert("TANK.HULL", &mut assets);
    let document = &hull.glb.document;
    assert_eq!(document.nodes.len(), 1);
    assert_eq!(
        (
            document.nodes[0].name.as_deref(),
            document.nodes[0].mesh.map(|mesh| mesh.value())
        ),
        (Some("tank.hull"), Some(0))
    );
    assert_eq!(places(&document.scenes[0].nodes), [0]);
    assert_eq!(document.materials[0].name.as_deref(), Some("tank.hull_0"));
    // A mesh of an empty container is named alone; its file is the language directory's, as
    // the other `Rock01.w3d` is no W3D file.
    let rock = convert("rock01", &mut assets);
    assert_eq!(rock.glb.document.nodes[0].name.as_deref(), Some("rock01"));
    // `NULL` and a box alone draw nothing; an emitter is not converted; a name of no file
    // and a name its file lacks are missing.
    let outcome = |name: &str, assets: &mut ModelAssets<'_>| match ModelImport::convert(
        name, assets,
    )
    .unwrap()
    {
        Converted::Model(_) => "model",
        Converted::Nothing => "nothing",
        Converted::Emitter => "emitter",
        Converted::Missing => "missing",
    };
    assert_eq!(outcome("Null", &mut assets), "nothing");
    assert_eq!(outcome("prop", &mut assets), "nothing");
    assert_eq!(outcome("spray", &mut assets), "emitter");
    assert_eq!(outcome("absent", &mut assets), "missing");
    assert_eq!(outcome("tank.ghost", &mut assets), "missing");
}

#[test]
fn a_zero_normal_takes_its_triangles_normals_by_area_or_up() {
    // Vertex 0 lies on a triangle of area 2 facing `z` and one of area 1 facing `x`. Vertex 4
    // lies on a triangle of no area: up.
    let positions = [
        [0.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [0.0, 2.0, 0.0],
        [0.0, 0.0, 1.0],
        [3.0, 3.0, 3.0],
    ];
    let triangles = [[0, 1, 2], [0, 2, 3], [4, 4, 4]];
    let mut normals = [
        [0.0; 3],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        [0.0; 3],
    ];
    ModelBuilder::fill_zero_normals(&positions, &triangles, &mut normals);
    // Each cross product is twice its triangle's area: (2, 0, 0) × (0, 2, 0) = (0, 0, 4) and
    // (0, 2, 0) × (0, 0, 1) = (2, 0, 0); their sum, (2, 0, 4), made unit by √20.
    let length = 20.0_f32.sqrt();
    assert_eq!(normals[0], [2.0 / length, 0.0, 4.0 / length]);
    assert_eq!(
        normals[1..4],
        [[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]]
    );
    assert_eq!(normals[4], [0.0, 1.0, 0.0]);
}

#[test]
fn vertex_colors_convert_only_as_the_diffuse_color() {
    // As `MeshMatDescClass` reads them: diffuse light, with ambient or not, takes the colors;
    // with emissive light, or with none of diffuse, they go elsewhere.
    let material = |ambient, diffuse, emissive| VertexMaterial {
        ambient,
        diffuse,
        emissive,
        opacity: 1.0,
    };
    let (on, off) = ([1, 0, 0], [0; 3]);
    let cases = [
        (vec![material(off, on, off)], true),
        (vec![material(on, on, off)], true),
        (vec![material(on, off, off)], false),
        (vec![material(off, on, on)], false),
        (vec![material(off, off, off)], false),
        // Each vertex's material counts: one emits, so the mesh's colors are not diffuse.
        (vec![material(off, on, off), material(off, off, on)], false),
    ];
    for (materials, diffuse) in cases {
        let ids = if materials.len() == 1 {
            Ids::One(0)
        } else {
            Ids::Each(vec![0, 1])
        };
        let mut mesh = W3dMesh {
            positions: vec![[0.0; 3]; 2],
            materials,
            ..W3dMesh::named("mesh")
        };
        mesh.passes[0].materials = ids;
        assert_eq!(
            ModelBuilder::colors_are_diffuse(&mesh, &mesh.passes[0]),
            diffuse,
            "{:?}",
            mesh.materials
        );
    }
}

#[test]
fn a_shader_of_no_primary_gradient_draws_its_texture_alone_unlit() {
    let test_dir = TestDir::new();
    fixture(&test_dir);
    let install = Install::open(&test_dir.path("zh")).unwrap();
    let assets = ModelAssets::new(&install).unwrap();
    // Two triangles of vertex colors, one of `Rock.tga`, which the archives hold, and one of
    // `Gone.tga`, which they do not; a diffuse of (255, 128, 0), half opaque, and `emissive`
    // light, which vertex colors allow only where no lit shader draws them.
    let mesh = |primary_gradient, emissive| W3dMesh {
        positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0; 3]],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        triangles: vec![[0, 1, 2], [2, 1, 3]],
        shaders: vec![Shader {
            primary_gradient,
            ..Shader::DEFAULT
        }],
        materials: vec![VertexMaterial {
            diffuse: [255, 128, 0],
            emissive,
            opacity: 0.5,
            ..VertexMaterial::DEFAULT
        }],
        textures: ["Rock.tga", "Gone.tga"]
            .map(|name| Texture {
                name: name.to_owned(),
                flags: 0,
            })
            .into(),
        passes: vec![Pass {
            materials: Ids::One(0),
            shaders: Ids::One(0),
            colors: Some(vec![[10, 20, 30, 40]; 4]),
            stage: Some(Stage {
                textures: Ids::Each(vec![0, 1]),
                coordinates: vec![[0.0; 2]; 4],
            }),
            detail: None,
        }],
        ..W3dMesh::named("decal")
    };
    let build = |primary_gradient, emissive| {
        let mut builder = ModelBuilder::new("decal");
        let mesh = mesh(primary_gradient, emissive);
        builder
            .mesh(&mesh, None, &ModelTextures::of([&mesh], &assets))
            .map(|_| builder.glb.document)
    };
    // Each material's base color, whether it is textured, its emissive light and whether it
    // is unlit; whether each primitive has vertex colors.
    let rows = |document: &Root| -> Vec<([f32; 4], bool, [f32; 3], bool)> {
        document
            .materials
            .iter()
            .map(|material| {
                let pbr = &material.pbr_metallic_roughness;
                (
                    pbr.base_color_factor.0,
                    pbr.base_color_texture.is_some(),
                    material.emissive_factor.0,
                    is_unlit(material),
                )
            })
            .collect()
    };
    let colored = |document: &Root| -> Vec<bool> {
        document.meshes[0]
            .primitives
            .iter()
            .map(|primitive| attribute(primitive, Semantic::Colors(0)).is_some())
            .collect()
    };
    // Lit, the texture times the diffuse color, half opaque; the missing texture's
    // `0x7FFF00FF` times them: (1, 0.216, 0, 0.5) × (1, 0, 1, 127 / 255).
    let half = f32::from(u16::try_from(srgb::light(128)).unwrap()) / 65_536.0;
    let lit = build(GRADIENT_MODULATE, [0; 3]).unwrap();
    assert_eq!(
        rows(&lit),
        [
            ([1.0, half, 0.0, 0.5], true, [0.0; 3], false),
            ([1.0, 0.0, 0.0, 0.5 * 127.0 / 255.0], false, [0.0; 3], false),
        ]
    );
    assert_eq!(colored(&lit), [true, true]);
    assert_eq!(
        build(GRADIENT_MODULATE, [255, 0, 0]).unwrap_err(),
        ModelError::VertexColors
    );
    assert_eq!(lit.extensions_used, ["KHR_materials_specular"]);
    // Unlit, the texture alone: white, opaque, of no emissive light, red as it is here, or
    // vertex colors; the missing texture's color alone.
    let unlit = build(0, [255, 0, 0]).unwrap();
    assert_eq!(
        rows(&unlit),
        [
            ([1.0; 4], true, [0.0; 3], true),
            ([1.0, 0.0, 1.0, 127.0 / 255.0], false, [0.0; 3], true),
        ]
    );
    assert_eq!(colored(&unlit), [false, false]);
    assert_eq!(
        unlit.extensions_used,
        ["KHR_materials_specular", "KHR_materials_unlit"]
    );
    // As glTF's JSON writes them: a specular factor of 0 beside the default color factor, and
    // the unlit extension an empty object.
    let json = Json::write(&unlit);
    assert!(
        json.contains(
            r#""KHR_materials_specular":{"specularFactor":0.0,"specularColorFactor":[1.0,1.0,1.0]}"#
        ),
        "{json}"
    );
    assert!(json.contains(r#""KHR_materials_unlit":{}"#), "{json}");
}

#[test]
fn a_triangle_of_no_texture_draws_what_its_shader_makes_of_the_null_vector() {
    let test_dir = TestDir::new();
    fixture(&test_dir);
    let install = Install::open(&test_dir.path("zh")).unwrap();
    let assets = ModelAssets::new(&install).unwrap();
    // Seven triangles of no texture, a diffuse of (255, 128, 0) and red emissive light, each
    // under its shader: texturing off; texturing on, opaque, by an alpha test and by alpha,
    // each modulating; adding, opaque; and two whose detail stages, of no texture either,
    // scale and add.
    let shaders = [
        Shader {
            texturing: false,
            ..Shader::DEFAULT
        },
        Shader::DEFAULT,
        Shader {
            alpha_test: true,
            ..Shader::DEFAULT
        },
        Shader {
            src_blend: 2,
            dest_blend: 5,
            depth_write: false,
            ..Shader::DEFAULT
        },
        // `GRADIENT_ADD`.
        Shader {
            primary_gradient: 2,
            ..Shader::DEFAULT
        },
        // `DETAILCOLOR_SCALE` and `DETAILCOLOR_ADD`.
        Shader {
            detail_color: 2,
            ..Shader::DEFAULT
        },
        Shader {
            detail_color: 4,
            ..Shader::DEFAULT
        },
    ];
    let mesh = W3dMesh {
        positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        normals: vec![[0.0, 0.0, 1.0]; 3],
        triangles: vec![[0, 1, 2]; 7],
        shaders: shaders.into(),
        materials: vec![VertexMaterial {
            diffuse: [255, 128, 0],
            emissive: [255, 0, 0],
            ..VertexMaterial::DEFAULT
        }],
        passes: vec![Pass {
            materials: Ids::One(0),
            shaders: Ids::Each(vec![0, 1, 2, 3, 4, 5, 6]),
            colors: None,
            stage: None,
            detail: Some(Stage {
                textures: Ids::One(Stage::NONE),
                coordinates: vec![[0.0; 2]; 3],
            }),
        }],
        ..W3dMesh::named("null")
    };
    let mut builder = ModelBuilder::new("null");
    builder
        .mesh(&mesh, None, &ModelTextures::of([&mesh], &assets))
        .unwrap();
    let document = &builder.glb.document;
    let rows: Vec<([f32; 4], AlphaMode, [f32; 3])> = document.meshes[0]
        .primitives
        .iter()
        .map(|primitive| {
            let material = &document.materials[primitive.material.unwrap().value()];
            (
                material.pbr_metallic_roughness.base_color_factor.0,
                *material.alpha_mode.as_ref().unwrap(),
                material.emissive_factor.0,
            )
        })
        .collect();
    // Texturing off draws the vertices' light, as ever. Modulation draws the null vector:
    // black and clear, which an opaque shader shows black, an alpha test discards and a
    // blend by alpha shows as nothing. Addition keeps the light, of alpha 0, which an opaque
    // shader shows whole. The two detail layers are the null vector, blended over their
    // first passes, so the scale makes them black and the add leaves them as they were.
    let half = f32::from(u16::try_from(srgb::light(128)).unwrap()) / 65_536.0;
    let lit = [1.0, half, 0.0, 1.0];
    assert_eq!(
        rows,
        [
            (lit, AlphaMode::Opaque, [1.0, 0.0, 0.0]),
            ([0.0; 4], AlphaMode::Opaque, [0.0; 3]),
            ([0.0; 4], AlphaMode::Mask, [0.0; 3]),
            ([0.0; 4], AlphaMode::Blend, [0.0; 3]),
            ([1.0, half, 0.0, 0.0], AlphaMode::Opaque, [1.0, 0.0, 0.0]),
            ([0.0; 4], AlphaMode::Opaque, [0.0; 3]),
            ([0.0; 4], AlphaMode::Opaque, [0.0; 3]),
            ([0.0; 4], AlphaMode::Blend, [0.0; 3]),
            ([0.0; 4], AlphaMode::Blend, [0.0; 3]),
        ]
    );
}

#[test]
fn each_pass_and_detail_stage_is_a_layer_over_the_one_before() {
    let test_dir = TestDir::new();
    fixture(&test_dir);
    let install = Install::open(&test_dir.path("zh")).unwrap();
    let assets = ModelAssets::new(&install).unwrap();
    let mut builder = ModelBuilder::new("layers");
    let stage = |coordinates: Vec<[f32; 2]>| Stage {
        textures: Ids::One(0),
        coordinates,
    };
    let pass = |shaders, colors, stage, detail| Pass {
        materials: Ids::One(0),
        shaders,
        colors,
        stage,
        detail,
    };
    // A glow of two triangles: its first pass opaque, of `Rock.tga`, by two shaders, whose
    // detail functions scale and add its detail stage; its second pass additive, writing
    // no depth. Each stage has its own coordinates.
    let opaque = |detail_color| Shader {
        detail_color,
        ..Shader::DEFAULT
    };
    let additive = Shader {
        dest_blend: 1,
        depth_write: false,
        ..Shader::DEFAULT
    };
    let glow_mesh = W3dMesh {
        positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0; 3]],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        triangles: vec![[0, 1, 2], [2, 1, 3]],
        // `DETAILCOLOR_SCALE` and `DETAILCOLOR_ADD`.
        shaders: vec![opaque(2), opaque(4), additive],
        textures: vec![Texture {
            name: "Rock.tga".to_owned(),
            flags: 0,
        }],
        passes: vec![
            pass(
                Ids::Each(vec![0, 1]),
                None,
                Some(stage(vec![[0.0, 0.0]; 4])),
                Some(stage(vec![[0.5, 0.25]; 4])),
            ),
            pass(Ids::One(2), None, Some(stage(vec![[2.0, 0.0]; 4])), None),
        ],
        ..W3dMesh::named("glow")
    };
    // A fade of one triangle: an opaque first pass of no texture, and a second by alpha,
    // writing no depth, of its own vertex colors and alphas, both of their shaders' texturing
    // off.
    let untextured = Shader {
        texturing: false,
        ..Shader::DEFAULT
    };
    let blended = Shader {
        src_blend: 2,
        dest_blend: 5,
        depth_write: false,
        ..untextured
    };
    let fade_mesh = W3dMesh {
        positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        normals: vec![[0.0, 0.0, 1.0]; 3],
        triangles: vec![[0, 1, 2]],
        shaders: vec![untextured, blended],
        passes: vec![
            pass(Ids::One(0), None, None, None),
            pass(Ids::One(1), Some(vec![[255, 255, 255, 64]; 3]), None, None),
        ],
        ..W3dMesh::named("fade")
    };
    let textures = ModelTextures::of([&glow_mesh, &fade_mesh], &assets);
    builder.mesh(&glow_mesh, None, &textures).unwrap();
    builder.mesh(&fade_mesh, None, &textures).unwrap();
    let document = &builder.glb.document;
    let [glow, fade] = [0, 1].map(|at| &document.meshes[at].primitives);
    // Each primitive of a mesh draws its one list of vertices.
    for primitives in [glow, fade] {
        let position = attribute(&primitives[0], Semantic::Positions);
        assert!(
            primitives
                .iter()
                .all(|primitive| attribute(primitive, Semantic::Positions) == position)
        );
    }
    // The glow: its first pass by shader, then its detail stage by shader, then its second
    // pass; each layer's coordinates its own stage's, `V` read as `1 − V`.
    let texcoords: Vec<Vec<f32>> = glow
        .iter()
        .map(|primitive| {
            builder
                .glb
                .floats_of(attribute(primitive, Semantic::TexCoords(0)).unwrap())
        })
        .collect();
    let uv = |u: f32, v: f32| [u, v].repeat(4);
    assert_eq!(
        texcoords,
        [
            uv(0.0, 1.0),
            uv(0.0, 1.0),
            uv(0.5, 0.75),
            uv(0.5, 0.75),
            uv(2.0, 1.0)
        ]
    );
    let triangles: Vec<usize> = glow
        .iter()
        .map(|primitive| {
            let indices = &document.accessors[primitive.indices.unwrap().value()];
            usize::try_from(indices.count.0).unwrap() / 3
        })
        .collect();
    assert_eq!(triangles, [1, 1, 1, 1, 2]);
    assert!(
        glow.iter()
            .all(|primitive| attribute(primitive, Semantic::Colors(0)).is_none())
    );
    // Each layer's material has its extras, of its blend and its depth bias over the one
    // under it: the detail stage's, white and unlit, multiplying for scale and adding for
    // add, 1 up; the second pass's, lit by its white vertex material, additive, 2 up. The
    // first pass's two say all in glTF.
    let document = &builder.glb.document;
    let material = |name: &str| {
        let at = document
            .materials
            .iter()
            .position(|material| material.name.as_deref() == Some(name));
        &document.materials[at.unwrap()]
    };
    let extras = |name: &str| extras(material(name));
    let unlit = |name: &str| is_unlit(material(name));
    let layer = |blend, depth_bias| MaterialExtras {
        blend,
        depth_write: false,
        depth_bias,
        team: TeamPaint::NONE,
    };
    let materials = places(glow.iter().flat_map(|primitive| &primitive.material));
    assert_eq!(materials, [0, 1, 2, 3, 4]);
    assert_eq!((extras("layers_0"), extras("layers_1")), (None, None));
    assert_eq!(extras("layers_2"), Some(layer(Blend::Multiply, 1)));
    assert_eq!(extras("layers_3"), Some(layer(Blend::Add, 1)));
    assert_eq!(extras("layers_4"), Some(layer(Blend::Add, 2)));
    let lit = ["layers_2", "layers_3", "layers_4"].map(unlit);
    assert_eq!(lit, [true, true, false]);
    // The fade: its first pass of no colors; its second of its own, alpha 64 of 255, by
    // alpha, 1 up.
    assert_eq!(attribute(&fade[0], Semantic::Colors(0)), None);
    let colors = builder
        .glb
        .colors_of(attribute(&fade[1], Semantic::Colors(0)).unwrap());
    assert_eq!(colors, [255, 255, 255, 64].repeat(3));
    assert_eq!(extras("layers_6"), Some(layer(Blend::Alpha, 1)));
    assert!(
        material("layers_6")
            .pbr_metallic_roughness
            .base_color_texture
            .is_none()
    );
    // A detail function no shipped shader uses is refused.
    let mut odd = glow_mesh;
    odd.shaders[0].detail_color = 3;
    assert_eq!(
        builder
            .mesh(&odd, None, &ModelTextures::of([&odd], &assets))
            .unwrap_err(),
        ModelError::DetailFunction(3)
    );
}

#[test]
fn an_hlod_of_a_pivot_its_hierarchy_lacks_or_of_two_nodes_of_one_name_is_refused() {
    let test_dir = TestDir::new();
    fixture(&test_dir);
    let install = Install::open(&test_dir.path("zh")).unwrap();
    let assets = ModelAssets::new(&install).unwrap();
    let pivot = |name: &str, parent| Pivot {
        name: name.to_owned(),
        parent,
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };
    let hierarchy = Hierarchy {
        name: "h".to_owned(),
        pivots: vec![pivot("root", None), pivot("h.part", Some(0))],
    };
    let mesh = W3dMesh {
        positions: vec![[0.0; 3]; 3],
        normals: vec![[0.0, 0.0, 1.0]; 3],
        triangles: vec![[0, 1, 2]],
        ..W3dMesh::named("h.part")
    };
    let build = |pivot, mesh: &W3dMesh| {
        let mut builder = ModelBuilder::new("h");
        let sub = SubObject {
            pivot,
            name: "h.part".to_owned(),
        };
        builder.hlod(
            &hierarchy,
            &[&sub],
            &[Some(mesh)],
            &ModelTextures::of([mesh], &assets),
        )
    };
    assert_eq!(build(2, &mesh).unwrap_err(), ModelError::Pivot);
    let empty = W3dMesh {
        triangles: Vec::new(),
        ..mesh.clone()
    };
    assert_eq!(
        build(1, &empty).unwrap_err(),
        ModelError::Empty("h.part".to_owned())
    );
    // The pivot `h.part` and the sub-object `h.part` are two nodes of one name.
    assert_eq!(
        build(0, &mesh).unwrap_err(),
        ModelError::NodeName("h.part".to_owned())
    );
}
