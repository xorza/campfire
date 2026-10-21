use super::*;
use crate::zero_hour::format::w3d::w3d_chunks::samples::chunk;

/// A textured skin of three vertices and two triangles, of every list the reader reads.
fn skin() -> W3dMesh {
    W3dMesh {
        flags: SKIN | TWO_SIDED,
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        normals: vec![[0.0, 0.0, 1.0]; 3],
        bones: Some(vec![0, 1, 1]),
        triangles: vec![[0, 1, 2], [2, 1, 0]],
        shaders: vec![Shader::DEFAULT],
        materials: vec![VertexMaterial {
            ambient: [40, 50, 60],
            diffuse: [10, 20, 30],
            emissive: [1, 2, 3],
            opacity: 0.5,
        }],
        textures: vec![Texture {
            name: "rock.tga".to_owned(),
            flags: 0x8,
        }],
        passes: vec![MaterialPass {
            materials: Indexing::One(0),
            shaders: Indexing::Each(vec![0, 0]),
            colors: Some(vec![[1, 2, 3, 4]; 3]),
            stage: Some(Stage {
                textures: Indexing::Each(vec![0, Stage::NONE]),
                coordinates: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            }),
            detail: None,
        }],
        ..W3dMesh::named("tank.skin")
    }
}

fn stage_of(mesh: &mut W3dMesh) -> &mut Stage {
    mesh.passes[0].stage.as_mut().unwrap()
}

/// The skin, changed by `change`, refused for `error`.
fn refused(change: impl FnOnce(&mut W3dMesh), error: W3dError) {
    let mut mesh = skin();
    change(&mut mesh);
    assert_eq!(W3dMesh::read(&mesh.chunk()[8..]), Err(error), "{mesh:?}");
}

#[test]
fn a_mesh_reads_as_written_and_its_lists_are_checked() {
    let mesh = skin();
    assert_eq!(W3dMesh::read(&mesh.chunk()[8..]).unwrap(), mesh);
    assert!(mesh.is_two_sided() && !mesh.is_hidden());
    let stage = mesh.passes[0].stage.as_ref().unwrap();
    assert_eq!([stage.texture(0), stage.texture(1)], [Some(0), None]);
    // An empty container's mesh is named by its own name alone; a stage may hold no
    // coordinates.
    let mut alone = W3dMesh {
        name: "rock01".to_owned(),
        ..skin()
    };
    stage_of(&mut alone).coordinates.clear();
    assert_eq!(W3dMesh::read(&alone.chunk()[8..]).unwrap(), alone);
    // A shader's primary gradient reads back; off, it draws its texture unlit, and with no
    // texture the vertices' light.
    let mut decal = skin();
    decal.shaders[0].primary_gradient = GRADIENT_DISABLE;
    assert_eq!(W3dMesh::read(&decal.chunk()[8..]).unwrap(), decal);
    assert!(decal.shaders[0].is_unlit() && !Shader::DEFAULT.is_unlit());
    let bare = Shader {
        texturing: false,
        ..decal.shaders[0]
    };
    assert!(!bare.is_unlit());

    refused(|mesh| mesh.normals.truncate(2), W3dError::Counts);
    refused(|mesh| mesh.positions[1][2] = f32::NAN, W3dError::NotFinite);
    refused(
        |mesh| mesh.materials[0].opacity = f32::NAN,
        W3dError::NotFinite,
    );
    refused(|mesh| mesh.bones = None, W3dError::Counts);
    refused(
        |mesh| mesh.flags = 0x0001_0000,
        W3dError::Geometry(0x0001_0000),
    );
    refused(|mesh| mesh.triangles[1] = [0, 1, 3], W3dError::Index);
    refused(
        |mesh| mesh.passes[0].materials = Indexing::One(1),
        W3dError::Index,
    );
    // One id is one for all, as `read_shader_ids` reads it; else one for each triangle.
    refused(
        |mesh| mesh.passes[0].shaders = Indexing::Each(vec![0; 3]),
        W3dError::Index,
    );
    refused(
        |mesh| mesh.passes[0].colors = Some(vec![[0; 4]; 2]),
        W3dError::Index,
    );
    // A triangle's texture `NONE` is untextured; one past the list, and `NONE` for all, are
    // refused, as `Peek_Texture` would read past its array.
    refused(
        |mesh| stage_of(mesh).textures = Indexing::Each(vec![0, 1]),
        W3dError::Index,
    );
    refused(
        |mesh| stage_of(mesh).textures = Indexing::One(Stage::NONE),
        W3dError::Index,
    );
    refused(
        |mesh| stage_of(mesh).coordinates.truncate(2),
        W3dError::Index,
    );
    assert_eq!(
        W3dMesh::read(&chunk(VERTICES, false, &[])),
        Err(W3dError::Missing)
    );
    // A mesh of no material pass: the skin's chunk without its last, the pass.
    let mesh = skin();
    let bytes = mesh.chunk();
    let pass = chunk(MATERIAL_PASS, true, &mesh.passes[0].chunks());
    assert_eq!(
        W3dMesh::read(&bytes[8..bytes.len() - pass.len()]),
        Err(W3dError::Missing)
    );
}

#[test]
fn a_pass_keeps_the_first_chunk_of_each_list_and_its_second_stage_as_its_detail() {
    // Two of each list: the second is the game's alternate material, and the first is kept;
    // the second stage is the next one, the detail stage, as `read_texture_stage` counts.
    let first = MaterialPass {
        materials: Indexing::One(1),
        shaders: Indexing::Each(vec![2, 3]),
        colors: Some(vec![[9; 4]]),
        stage: Some(Stage {
            textures: Indexing::One(4),
            coordinates: vec![[0.5, 0.25]],
        }),
        detail: None,
    };
    let second = MaterialPass {
        materials: Indexing::One(5),
        shaders: Indexing::One(6),
        colors: Some(vec![[7; 4]]),
        stage: Some(Stage {
            textures: Indexing::One(8),
            coordinates: vec![[1.0, 1.0]],
        }),
        detail: None,
    };
    let mut body = first.chunks();
    body.extend(second.chunks());
    let detailed = MaterialPass {
        detail: second.stage.clone(),
        ..first.clone()
    };
    assert_eq!(W3dMesh::pass(&body).unwrap(), detailed);
    // A third stage, which the game cannot hold, is refused.
    body.extend(second.stage.as_ref().unwrap().chunk());
    assert_eq!(W3dMesh::pass(&body), Err(W3dError::Stages));
    // Within one stage, the first texture ids and the first coordinates.
    let coordinates = |u: f32, v: f32| {
        chunk(
            STAGE_COORDINATES,
            false,
            &[u.to_le_bytes(), v.to_le_bytes()].concat(),
        )
    };
    let doubled = [
        chunk(TEXTURE_IDS, false, &4_u32.to_le_bytes()),
        chunk(TEXTURE_IDS, false, &8_u32.to_le_bytes()),
        coordinates(0.5, 0.25),
        coordinates(1.0, 1.0),
    ]
    .concat();
    assert_eq!(
        W3dMesh::pass(&chunk(TEXTURE_STAGE, true, &doubled))
            .unwrap()
            .stage,
        first.stage
    );
    // A pass of no lists has id 0 for all.
    let empty = W3dMesh::pass(&[]).unwrap();
    assert_eq!(
        (empty.materials, empty.shaders, empty.colors, empty.stage),
        (Indexing::One(0), Indexing::One(0), None, None)
    );
}
