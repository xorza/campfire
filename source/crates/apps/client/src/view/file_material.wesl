// A model's material: the standard material's color, lit unless it says it is unlit, as Bevy's
// own shader draws it, and not drawn where its alpha is below its cutoff, as an alpha test is; its
// blend and depth write are its pipeline's. Its mesh's tag names two places in the match's colors,
// each from 1, 0 for none: its team's, in its low 16 bits, which its team paint colors it by, and
// its tint's, in its high 16, which adds to its light as Zero Hour's drawable tint does.
import bevy_pbr::render::{
    mesh_functions::get_tag,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
};

@if(PREPASS_PIPELINE)
import bevy_pbr::{
    prepass::io::{VertexOutput, FragmentOutput},
    deferred::functions::deferred_output,
};
@else
import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
};

struct FileBlend {
    cutoff: f32,
    team: u32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> file_blend: FileBlend;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var<storage, read> colors: array<vec4<f32>>;

const TINT: u32 = 1u;
const HUE_MASK: u32 = 2u;
// Below this, half a step of 8 bits under one, a texel's alpha marks its team's hue.
const MASKED: f32 = 1.0 - 0.5 / 255.0;

// sRGB's transfer, both ways, as the game's texels are recolored in sRGB.
fn to_srgb(linear: vec3<f32>) -> vec3<f32> {
    let low = linear * 12.92;
    let high = 1.055 * pow(linear, vec3(1.0 / 2.4)) - 0.055;
    return select(high, low, linear <= vec3(0.0031308));
}

fn to_linear(srgb: vec3<f32>) -> vec3<f32> {
    let low = srgb / 12.92;
    let high = pow((srgb + 0.055) / 1.055, vec3(2.4));
    return select(high, low, srgb <= vec3(0.04045));
}

fn to_hsv(c: vec3<f32>) -> vec3<f32> {
    let most = max(c.r, max(c.g, c.b));
    let least = min(c.r, min(c.g, c.b));
    let span = most - least;
    var hue = 0.0;
    if span > 0.0 {
        if most == c.r {
            hue = (c.g - c.b) / span;
        } else if most == c.g {
            hue = 2.0 + (c.b - c.r) / span;
        } else {
            hue = 4.0 + (c.r - c.g) / span;
        }
        hue = fract(hue / 6.0);
    }
    let saturation = select(0.0, span / most, most > 0.0);
    return vec3(hue, saturation, most);
}

fn to_rgb(hsv: vec3<f32>) -> vec3<f32> {
    let k = vec3(0.0, 4.0, 2.0);
    let p = clamp(abs(fract(hsv.x + k / 6.0) * 6.0 - 3.0) - 1.0, vec3(0.0), vec3(1.0));
    return hsv.z * mix(vec3(1.0), p, hsv.y);
}

// `color` of the team's hue, and of its saturation times the team's, as Zero Hour shifts a texel's
// hue in sRGB.
fn hue_shift(color: vec3<f32>, team: vec3<f32>) -> vec3<f32> {
    let own = to_hsv(to_srgb(color));
    let theirs = to_hsv(to_srgb(team));
    return to_linear(to_rgb(vec3(theirs.x, own.y * theirs.y, own.z)));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let tag = get_tag(in.instance_index);
    let team_place = tag & 0xffffu;
    var base = pbr_input.material.base_color;
    if team_place != 0u {
        let team = colors[team_place - 1u].rgb;
        if (file_blend.team & TINT) != 0u {
            base = vec4(base.rgb * team, base.a);
        }
        if (file_blend.team & HUE_MASK) != 0u && base.a < MASKED {
            base = vec4(hue_shift(base.rgb, team), base.a);
        }
    }
    // A hue mask's alpha is no see-through.
    if (file_blend.team & HUE_MASK) != 0u {
        base.a = 1.0;
    }
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        base,
    );
    if pbr_input.material.base_color.a < file_blend.cutoff {
        discard;
    }
@if(PREPASS_PIPELINE)
    let out = deferred_output(in, pbr_input);
@if(!PREPASS_PIPELINE)
    var out: FragmentOutput;
@if(!PREPASS_PIPELINE) {
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    let tint_place = tag >> 16u;
    if tint_place != 0u {
        let tint = colors[tint_place - 1u].rgb;
        out.color = vec4(out.color.rgb + pbr_input.material.base_color.rgb * tint, out.color.a);
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
}
    return out;
}
