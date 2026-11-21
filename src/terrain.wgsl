struct Camera {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    eye: vec4<f32>,
    target_block: vec4<f32>,
    atmosphere: vec4<f32>,
}

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var tile_sampler: sampler;
@group(1) @binding(1) var tiles: texture_2d_array<f32>;

const SUN_DIRECTION = vec3<f32>(-0.458, 0.795, -0.397);
const SKY_HORIZON = vec3<f32>(0.63, 0.78, 0.80);
const SKY_ZENITH = vec3<f32>(0.16, 0.39, 0.66);

fn sky_gradient(elevation: f32) -> vec3<f32> {
    return mix(SKY_HORIZON, SKY_ZENITH, pow(max(0.0, elevation), 0.52));
}

struct TerrainVertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) layer: u32,
    @location(4) light: f32,
}

struct TerrainVarying {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) relative_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) layer: u32,
    @location(4) light: f32,
}

@vertex
fn terrain_vertex(vertex: TerrainVertex) -> TerrainVarying {
    return TerrainVarying(
        camera.view_projection * vec4<f32>(vertex.position, 1.0),
        vertex.position - camera.eye.xyz,
        vertex.normal,
        vertex.uv,
        vertex.layer,
        vertex.light,
    );
}

@fragment
fn terrain_fragment(input: TerrainVarying) -> @location(0) vec4<f32> {
    let albedo = textureSample(tiles, tile_sampler, input.uv, i32(input.layer)).rgb;
    let normal = input.normal;
    let sun = max(dot(normal, SUN_DIRECTION), 0.0);
    let bounce = 0.59 + 0.12 * normal.y;
    let light = bounce * vec3<f32>(0.85, 0.94, 1.02) + sun * vec3<f32>(0.43, 0.37, 0.28);
    let lit = albedo * light * input.light;
    let air = sky_gradient(normalize(input.relative_position).y);
    let fog = smoothstep(camera.atmosphere.x, camera.atmosphere.y, length(input.relative_position.xz));
    return vec4<f32>(mix(lit, air, fog), 1.0);
}

struct SkyVarying {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

@vertex
fn sky_vertex(@builtin(vertex_index) vertex: u32) -> SkyVarying {
    let corners = array(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    let ndc = corners[vertex];
    return SkyVarying(vec4<f32>(ndc, 1.0, 1.0), ndc);
}

fn sky_hash(cell: vec2<f32>) -> f32 {
    let p = fract(vec3<f32>(cell.xyx) * 0.1031);
    let q = p + dot(p, p.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn cloud_noise(point: vec2<f32>) -> f32 {
    let cell = floor(point);
    let local = fract(point);
    let t = local * local * (3.0 - 2.0 * local);
    let lower = mix(sky_hash(cell), sky_hash(cell + vec2<f32>(1.0, 0.0)), t.x);
    let upper = mix(sky_hash(cell + vec2<f32>(0.0, 1.0)), sky_hash(cell + vec2<f32>(1.0)), t.x);
    return mix(lower, upper, t.y);
}

@fragment
fn sky_fragment(input: SkyVarying) -> @location(0) vec4<f32> {
    let point = camera.inverse_view_projection * vec4<f32>(input.ndc, 1.0, 1.0);
    let direction = normalize(point.xyz / point.w - camera.eye.xyz);
    var color = sky_gradient(direction.y);
    let sun_angle = clamp(dot(direction, SUN_DIRECTION), 0.0, 1.0);
    let sun_glow = 0.19 * pow(sun_angle, 14.0) + 0.65 * smoothstep(0.9991, 0.9994, sun_angle);
    color += sun_glow * vec3<f32>(1.0, 0.73, 0.39);

    if direction.y > 0.028 {
        let cloud_plane = max(34.0, 176.0 - camera.eye.y);
        let cloud_point = camera.eye.xz + direction.xz * cloud_plane / direction.y;
        let p = cloud_point * vec2<f32>(0.023, 0.036) + vec2<f32>(7.6, -12.0);
        let cover = cloud_noise(p) * 0.76 + cloud_noise(2.1 * p) * 0.24;
        let opacity = smoothstep(0.54, 0.665, cover) * smoothstep(0.03, 0.21, direction.y);
        let cloud_color = mix(vec3<f32>(0.76, 0.84, 0.88), vec3<f32>(0.99, 0.99, 0.94), cover);
        color = mix(color, cloud_color, opacity * 0.89);
    }
    return vec4<f32>(color, 1.0);
}

@vertex
fn outline_vertex(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {
    let corners = array(
        vec3<f32>(-0.004, -0.004, -0.004), vec3<f32>(1.004, -0.004, -0.004),
        vec3<f32>(1.004, -0.004, 1.004), vec3<f32>(-0.004, -0.004, 1.004),
        vec3<f32>(-0.004, 1.004, -0.004), vec3<f32>(1.004, 1.004, -0.004),
        vec3<f32>(1.004, 1.004, 1.004), vec3<f32>(-0.004, 1.004, 1.004),
    );
    let edges = array<u32, 24>(0, 1, 1, 2, 2, 3, 3, 0, 4, 5, 5, 6, 6, 7, 7, 4, 0, 4, 1, 5, 2, 6, 3, 7);
    let point = camera.target_block.xyz + corners[edges[vertex]];
    return camera.view_projection * vec4<f32>(point, 1.0);
}

@fragment
fn outline_fragment() -> @location(0) vec4<f32> {
    return vec4<f32>(0.012, 0.022, 0.026, 0.92);
}
