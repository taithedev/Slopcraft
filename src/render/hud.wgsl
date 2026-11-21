@group(0) @binding(0) var tile_sampler: sampler;
@group(0) @binding(1) var tiles: texture_2d_array<f32>;
@group(1) @binding(0) var font: texture_2d<f32>;

struct HudVertex {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) kind: u32,
    @location(4) layer: u32,
}

struct HudVarying {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) kind: u32,
    @location(3) @interpolate(flat) layer: u32,
}

@vertex
fn hud_vertex(vertex: HudVertex) -> HudVarying {
    let linear = pow((vertex.color.rgb + 0.055) / 1.055, vec3<f32>(2.4));
    let color = select(linear, vertex.color.rgb / 12.92, vertex.color.rgb <= vec3<f32>(0.04045));
    return HudVarying(vec4<f32>(vertex.position, 0.0, 1.0), vertex.uv, vec4<f32>(color, vertex.color.a), vertex.kind, vertex.layer);
}

@fragment
fn hud_fragment(input: HudVarying) -> @location(0) vec4<f32> {
    switch input.kind {
        case 1u: {
            let glyph = textureLoad(font, vec2<i32>(input.uv), 0).r;
            return vec4<f32>(input.color.rgb, input.color.a * glyph);
        }
        case 2u: {
            return input.color * textureSampleLevel(tiles, tile_sampler, input.uv, i32(input.layer), 0.0);
        }
        default: {
            return input.color;
        }
    }
}
