# Model materials — research

What the imported models' materials need past Bevy's standard material, and what Bevy's built-in modes draw, for [Team colors in the one material](../design/02-engine-core.md#client). Facts only.

## The Zero Hour import

Counted on 20 October 2026 in the package the Zero Hour check imports from the Steam install of Zero Hour and Generals: 1,724 models, 7,104 glTF materials, read from each model's JSON.

| Material | Count |
| --- | --- |
| No extras: glTF's own opaque, mask or blend says all | 5,493 |
| Extras of an opaque blend: a depth bias, a team paint, or both | 492 |
| Extras of a mask | 1 |
| Extras of an alpha blend | 306 |
| An alpha blend with an alpha test (`alpha_mask`) | 36 |
| An additive blend (`add`) | 757 |
| … of a base color's alpha below 1 | 50 |
| A multiplying blend (`multiply`) | 15 |
| A screen blend (`screen`) | 4 |
| A blend that writes depth | 36 |
| A depth bias | 701 |
| A team tint (`HOUSECOLOR`) | 449 |

- **Models a team paints.** 417 models hold a material of team paint.
- **Textures a team's hue recolors.** 37 textures of hue masks (`ZHCA`), each `VK_FORMAT_R8G8B8A8_SRGB` with its full chain of levels, 873,801 bytes for one copy of all 37.

## Bevy 0.20

Read from `bevy_material` 0.20.0 and `bevy_pbr` 0.20.0 on 20 October 2026.

- **The alpha modes.** `AlphaMode` is `Opaque`, `Mask(cutoff)`, `Blend`, `Premultiplied`, `AlphaToCoverage`, `Add` and `Multiply` (`bevy_material/src/alpha.rs`). No mode blends after an alpha test, and none is a screen blend.
- **Add and multiply weigh by alpha.** The standard material's shader premultiplies the color of an `Add` material, `vec4(color.rgb * color.a, 0.0)`, before a blend of one and one (`premultiply_alpha`, `bevy_pbr/src/render/pbr_functions.wesl`); a blend of one and one of the color alone, as D3D's `ONE, ONE`, is the same only at alpha 1.
- **Depth writes follow the mode.** `StandardMaterial` has no depth write of its own: an opaque or masked material writes depth, a blended one does not.
- **What the standard material holds.** `depth_bias`, `unlit`, `emissive` and `base_color` are its fields, so a layer's order, an unlit material, a light added over a model and a color times the texture need no extension.
