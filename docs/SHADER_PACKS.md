# Shader packs

Slopcraft loads external WGSL source and creates replacement GPU pipelines for terrain, the sky, and the targeted-block outline. Press F7 to cycle through installed packs and the built-in shader. Press F6 to reread the selected pack after editing its file.

Put a pack in the game's data directory with this layout. The default Windows data directory is `%LOCALAPPDATA%\Slopcraft`.

```text
shaderpacks/
  amber/
    terrain.wgsl
    pack.json
```

The game installs the included `amber` example on its first start. Its source also ships in the repository's `shaderpacks/amber` folder. Start from its `terrain.wgsl` when creating another pack. The `pack.json` file describes the example for people reading the directory. The renderer loads the WGSL file directly. Each source file must be UTF-8 and at most 2 MiB.

The game validates the shader module and all three replacement pipelines before activating them. A failed reload reports an error and retains the last working pipelines. A missing or invalid startup pack can be replaced with the built-in selection. Compiler details appear in the console.

## World shader interface

The source file supplies these entry points. Keep the entry-point names and vertex input layouts. Function-internal varyings can change together as long as each vertex/fragment pair remains compatible.

| Entry point | Stage | Inputs | Render behavior |
| --- | --- | --- | --- |
| `terrain_vertex` | Vertex | Vertex attributes below | Indexed triangle list, counterclockwise front faces, back-face culling |
| `terrain_fragment` | Fragment | Output of `terrain_vertex` | Opaque linear RGBA color at location 0 |
| `sky_vertex` | Vertex | `@builtin(vertex_index)` values 0 through 2 | One oversized screen triangle, no vertex buffer |
| `sky_fragment` | Fragment | Output of `sky_vertex` | Opaque linear RGBA color at location 0 |
| `outline_vertex` | Vertex | `@builtin(vertex_index)` values 0 through 23 | Twelve lines, no vertex buffer |
| `outline_fragment` | Fragment | Output of `outline_vertex` | Linear RGBA color at location 0 with alpha blending |

The renderer uses a right-handed camera with positive Y pointing upward. The projection follows WebGPU depth coordinates from 0 through 1. Terrain writes depth using the `Less` comparison. The sky uses `Equal` with clip-space Z equal to W so it fills the untouched far plane. The outline uses `LessEqual` and leaves depth unchanged.

### Bind group 0

Binding 0 is a uniform buffer visible to vertex and fragment stages. Match this exact 176-byte layout.

```wgsl
struct Camera {
    view_projection: mat4x4<f32>,         // byte 0
    inverse_view_projection: mat4x4<f32>, // byte 64
    eye: vec4<f32>,                      // byte 128, world-space XYZ and W=1
    target_block: vec4<f32>,             // byte 144, targeted block origin XYZ
    atmosphere: vec4<f32>,               // byte 160, fog start, fog end, reserved, reserved
}
@group(0) @binding(0) var<uniform> camera: Camera;
```

The sky and outline pipelines have access to bind group 0. Keep their entry points independent of the material bindings in group 1.

### Bind group 1

Terrain fragment shaders can sample the original material tile array.

```wgsl
@group(1) @binding(0) var tile_sampler: sampler;
@group(1) @binding(1) var tiles: texture_2d_array<f32>;
```

The texture uses `Rgba8UnormSrgb`, so sampling decodes its RGB values into linear space. The sampler repeats in U and V, uses nearest texel filtering, and interpolates between generated mip levels. Greedy quads have UVs greater than 1 to repeat a tile once per block. Layer numbers come from the current block registry and can change when mods change the material list. Use each vertex's layer value to sample the assigned material.

### Terrain vertex buffer

The stride is 40 bytes. Attributes use these exact formats and offsets.

| Location | WGSL field | GPU format | Offset | Meaning |
| --- | --- | --- | --- | --- |
| 0 | `position: vec3<f32>` | Float32x3 | 0 | World-space position |
| 1 | `normal: vec3<f32>` | Float32x3 | 12 | Axis-aligned outward normal |
| 2 | `uv: vec2<f32>` | Float32x2 | 24 | Repeating material coordinates |
| 3 | `layer: u32` | Uint32 | 32 | Texture-array layer |
| 4 | `light: f32` | Float32 | 36 | Baked corner ambient occlusion, approximately 0.78 to 1 |

Integer values passed to the fragment shader require `@interpolate(flat)`. Color outputs are linear because the window target performs sRGB encoding. Fog can use `camera.atmosphere.xy` to stay aligned with the configured render distance. The example applies fog to horizontal distance and colors it using the sky gradient.

## Current scope

Packs can replace world vertex and fragment code using the existing textures and camera data. They can alter lighting, fog, sky, outlines, and geometry in the supplied vertex stages. The HUD uses its own renderer. Additional textures, screen-space postprocessing, shadow maps, extra render passes, and pack-specific uniforms require renderer changes.
