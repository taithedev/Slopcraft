use bytemuck::{Pod, Zeroable};
use glam::IVec3;

use crate::{
    blocks::{AIR, BlockId, ItemIcon, Pattern, SLATE, TextureSpec, Tile, block_def, registry},
    world::{CHUNK_SIZE, ChunkPos, WORLD_HEIGHT, World},
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub layer: u32,
    pub light: f32,
}

#[derive(Default)]
pub struct ChunkMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

pub struct TexturePack {
    pub size: u32,
    pub layers: u32,
    pub rgba: Vec<u8>,
}

const TILE_SIZE: u32 = 16;
const PADDED_WIDTH: i32 = CHUNK_SIZE + 2;
const PADDED_HEIGHT: i32 = WORLD_HEIGHT + 2;
const PADDED_VOLUME: usize = (PADDED_WIDTH * PADDED_WIDTH * PADDED_HEIGHT) as usize;

struct ChunkSamples {
    blocks: Vec<BlockId>,
    height: i32,
}

impl ChunkSamples {
    fn new(world: &World, origin: IVec3) -> Self {
        let mut blocks = vec![AIR; PADDED_VOLUME];
        let mut height = 0;
        for z in -1..=CHUNK_SIZE {
            for x in -1..=CHUNK_SIZE {
                blocks[Self::index(IVec3::new(x, -1, z))] = SLATE;
            }
        }
        for y in 0..WORLD_HEIGHT {
            for z in -1..=CHUNK_SIZE {
                for x in -1..=CHUNK_SIZE {
                    let local = IVec3::new(x, y, z);
                    let id = world.block(origin + local);
                    blocks[Self::index(local)] = id;
                    if id != AIR && (0..CHUNK_SIZE).contains(&x) && (0..CHUNK_SIZE).contains(&z) {
                        height = y + 1;
                    }
                }
            }
        }
        Self { blocks, height }
    }

    #[inline]
    fn index(p: IVec3) -> usize {
        (((p.y + 1) * PADDED_WIDTH + p.z + 1) * PADDED_WIDTH + p.x + 1) as usize
    }

    #[inline]
    fn block(&self, p: IVec3) -> BlockId {
        self.blocks[Self::index(p)]
    }

    #[inline]
    fn solid(&self, p: IVec3) -> bool {
        block_def(self.block(p)).opaque
    }

    fn corner_shade(&self, outside: IVec3, u: IVec3, v: IVec3) -> u8 {
        let side_u = self.solid(outside + u);
        let side_v = self.solid(outside + v);
        if side_u && side_v {
            return 3;
        }
        u8::from(side_u) + u8::from(side_v) + u8::from(self.solid(outside + u + v))
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct Face {
    layer: u32,
    shade: [u8; 4],
}

impl Face {
    fn mergeable(self) -> bool {
        self.shade.iter().all(|shade| *shade == self.shade[0])
    }
}

#[derive(Clone, Copy)]
struct FaceDirection {
    axis: usize,
    u_axis: usize,
    v_axis: usize,
    sign: i32,
}

const DIRECTIONS: [FaceDirection; 6] = [
    FaceDirection::new(0, 2, 1, 1),
    FaceDirection::new(0, 2, 1, -1),
    FaceDirection::new(1, 0, 2, 1),
    FaceDirection::new(1, 0, 2, -1),
    FaceDirection::new(2, 0, 1, 1),
    FaceDirection::new(2, 0, 1, -1),
];

impl FaceDirection {
    const fn new(axis: usize, u_axis: usize, v_axis: usize, sign: i32) -> Self {
        Self {
            axis,
            u_axis,
            v_axis,
            sign,
        }
    }

    fn push_quad(
        self,
        mesh: &mut ChunkMesh,
        mut corner: IVec3,
        width: i32,
        height: i32,
        face: Face,
    ) {
        let first = mesh.vertices.len() as u32;
        let normal = IVec3::AXES[self.axis] * self.sign;
        let u = IVec3::AXES[self.u_axis];
        let v = IVec3::AXES[self.v_axis];
        corner[self.axis] += i32::from(self.sign > 0);
        let positions = [
            corner,
            corner + u * width,
            corner + u * width + v * height,
            corner + v * height,
        ];
        let uv = [
            [0.0, height as f32],
            [width as f32, height as f32],
            [width as f32, 0.0],
            [0.0, 0.0],
        ];
        for corner_index in 0..4 {
            mesh.vertices.push(Vertex {
                position: positions[corner_index].as_vec3().to_array(),
                normal: normal.as_vec3().to_array(),
                uv: uv[corner_index],
                layer: face.layer,
                light: 1.0 - f32::from(face.shade[corner_index]) * 0.073,
            });
        }
        // Split along the brighter diagonal to reduce visible AO anisotropy.
        let [a, b, c, d] = face.shade;
        let mut indices = if a + c > b + d {
            [0, 1, 3, 1, 2, 3]
        } else {
            [0, 1, 2, 0, 2, 3]
        };
        if u.cross(v).dot(normal) < 0 {
            indices.swap(1, 2);
            indices.swap(4, 5);
        }
        mesh.indices.extend(indices.map(|index| first + index));
    }
}

pub fn build_chunk(world: &World, pos: ChunkPos) -> ChunkMesh {
    let origin = IVec3::new(pos.x * CHUNK_SIZE, 0, pos.z * CHUNK_SIZE);
    let samples = ChunkSamples::new(world, origin);
    let lengths = [CHUNK_SIZE, samples.height, CHUNK_SIZE];
    let mut mesh = ChunkMesh::default();
    let longest_slice = CHUNK_SIZE * CHUNK_SIZE.max(samples.height);
    let mut mask = vec![None; longest_slice as usize];

    for direction in DIRECTIONS {
        let FaceDirection {
            axis,
            u_axis,
            v_axis,
            sign,
        } = direction;
        let u_axis_vec = IVec3::AXES[u_axis];
        let v_axis_vec = IVec3::AXES[v_axis];
        let normal = IVec3::AXES[axis] * sign;
        let width = lengths[u_axis] as usize;
        let height = lengths[v_axis] as usize;
        for slice in 0..lengths[axis] {
            let slice_origin = IVec3::AXES[axis] * slice;
            for v in 0..height {
                for u in 0..width {
                    let p = slice_origin + u_axis_vec * u as i32 + v_axis_vec * v as i32;
                    let block = block_def(samples.block(p));
                    let outside = p + normal;
                    let cell = &mut mask[v * width + u];
                    *cell = None;
                    if block.id == AIR || samples.solid(outside) {
                        continue;
                    }
                    *cell = Some(Face {
                        layer: block.face_layer(normal.y),
                        shade: [
                            samples.corner_shade(outside, -u_axis_vec, -v_axis_vec),
                            samples.corner_shade(outside, u_axis_vec, -v_axis_vec),
                            samples.corner_shade(outside, u_axis_vec, v_axis_vec),
                            samples.corner_shade(outside, -u_axis_vec, v_axis_vec),
                        ],
                    });
                }
            }
            for v in 0..height {
                for u in 0..width {
                    let Some(face) = mask[v * width + u] else {
                        continue;
                    };
                    let mut span_u = 1;
                    let mut span_v = 1;
                    if face.mergeable() {
                        while u + span_u < width && mask[v * width + u + span_u] == Some(face) {
                            span_u += 1;
                        }
                        while v + span_v < height {
                            let start = (v + span_v) * width + u;
                            if mask[start..start + span_u]
                                .iter()
                                .any(|cell| *cell != Some(face))
                            {
                                break;
                            }
                            span_v += 1;
                        }
                    }
                    for row in v..v + span_v {
                        let start = row * width + u;
                        mask[start..start + span_u].fill(None);
                    }
                    direction.push_quad(
                        &mut mesh,
                        origin + slice_origin + u_axis_vec * u as i32 + v_axis_vec * v as i32,
                        span_u as i32,
                        span_v as i32,
                        face,
                    );
                }
            }
        }
    }
    mesh
}

pub fn generate_textures() -> TexturePack {
    let textures = &registry().textures;
    let mut rgba = Vec::with_capacity((TILE_SIZE * TILE_SIZE * textures.len() as u32 * 4) as usize);
    for tile in textures {
        for y in 0..TILE_SIZE {
            for x in 0..TILE_SIZE {
                let pixel = match (tile.legacy, tile.icon) {
                    (Some(legacy), _) => tile_pixel(legacy, x, y),
                    (_, Some(icon)) => item_pixel(tile, icon, x, y),
                    _ => material_pixel(tile, x, y),
                };
                rgba.extend_from_slice(&pixel);
            }
        }
    }
    TexturePack {
        size: TILE_SIZE,
        layers: textures.len() as u32,
        rgba,
    }
}

fn material_pixel(tile: &TextureSpec, x: u32, y: u32) -> [u8; 4] {
    let rgb = tile.color.map(i32::from);
    let noise = texture_noise(x, y, tile.seed);
    let grain = (noise % 13) as i32 - 6;
    let patch = (texture_noise(x / 3, y / 3, tile.seed) % 17) as i32 - 8;
    let bevel = |edge: bool, highlight: bool| {
        if edge {
            -27
        } else if highlight {
            15
        } else {
            0
        }
    };
    let variation = match tile.pattern {
        Pattern::Solid => grain / 3,
        Pattern::Stone => grain + patch + if noise % 41 == 0 { 18 } else { 0 },
        Pattern::Cobble => {
            let offset = (y / 5 % 2) * 3;
            grain
                + patch / 2
                + bevel(
                    (x + offset) % 7 == 0 || y % 5 == 0,
                    (x + offset) % 7 == 1 || y % 5 == 1,
                )
        }
        Pattern::Bricks => {
            let stagger = (y / 4 % 2) * 4;
            grain / 2 + patch / 2 + bevel((x + stagger) % 8 == 0 || y % 4 == 0, y % 4 == 1)
        }
        Pattern::Tiles => {
            let edge = x % 8 == 0 || y % 8 == 0;
            grain / 2 + bevel(edge, x % 8 == 1 || y % 8 == 1)
        }
        Pattern::Carved => {
            let edge = x == 2
                || x == 13
                || y == 2
                || y == 13
                || ((5..=10).contains(&x) && (y == 5 || y == 10));
            grain / 2 + bevel(edge, x == 3 || y == 3)
        }
        Pattern::Planks => {
            let seam = y % 4 == 0 || (x + (y / 4) * 5) % 16 == 0;
            grain / 2 + patch / 2 + bevel(seam, y % 4 == 1)
        }
        Pattern::Parquet => {
            let (u, v) = if (x / 8 + y / 8) % 2 == 0 {
                (x, y)
            } else {
                (y, x)
            };
            grain / 2 + bevel(v % 4 == 0 || u % 8 == 0, v % 4 == 1)
        }
        Pattern::Mosaic => {
            let line = (x + y) % 8 == 0 || (x + 16 - y) % 8 == 0;
            grain / 2 + patch / 2 + bevel(line, (x + y) % 8 == 1)
        }
        Pattern::Bark => {
            let channel = (x + y / 5) % 5;
            grain
                + patch / 2
                + match channel {
                    0 => -35,
                    1 => 9,
                    _ => -10,
                }
        }
        Pattern::Rings => {
            let distance = x.abs_diff(8).max(y.abs_diff(7)) + x.abs_diff(8).min(y.abs_diff(7)) / 3;
            grain
                + match distance {
                    0 => -25,
                    2 | 5 => -15,
                    7.. => -40,
                    _ => 10,
                }
        }
        Pattern::Leaves => {
            grain
                + patch
                + match (x + y / 2) % 4 {
                    0 => 14,
                    3 => -18,
                    _ => 0,
                }
        }
        Pattern::Sand => grain / 2 + patch / 3 + if noise % 31 == 0 { -12 } else { 0 },
        Pattern::Dirt => grain + patch / 2 + if noise % 19 == 0 { -17 } else { 0 },
        Pattern::Fabric => grain / 3 + if (x + y) % 2 == 0 { 6 } else { -6 },
        Pattern::Ceramic => grain / 3 + bevel(x == 0 || y == 15, x == 1 || y == 1),
        Pattern::Metal => {
            grain / 2
                + if y == 1 || x == 1 {
                    18
                } else if y == 14 || x == 14 {
                    -20
                } else {
                    ((x + y) % 8) as i32 / 2
                }
        }
        Pattern::Ore => {
            let cluster = texture_noise(x / 3, y / 3, tile.seed) % 5 == 0;
            if !cluster {
                return tone([115, 126, 133], grain + patch);
            }
            grain + 14
        }
        Pattern::Marble => {
            grain / 2
                + if (x + y / 2 + (noise % 3)) % 9 < 2 {
                    -19
                } else {
                    3
                }
        }
        Pattern::Crystal => grain / 3 + if (x + y) % 8 < 3 { 24 } else { -9 },
        Pattern::Glass => {
            let diagonal = x + y;
            grain / 3
                + if diagonal == 5 || diagonal == 6 || diagonal == 11 {
                    32
                } else if x == 0 || y == 15 {
                    -22
                } else {
                    0
                }
        }
    };
    tone(rgb, variation)
}

fn item_pixel(tile: &TextureSpec, icon: ItemIcon, x: u32, y: u32) -> [u8; 4] {
    let handle = x.abs_diff(7) <= 1 && (6..=13).contains(&y);
    let tool_head = match icon {
        ItemIcon::Pickaxe => {
            (3..=12).contains(&x) && (3..=4).contains(&y) || (x == 12 && (5..=6).contains(&y))
        }
        ItemIcon::Hammer => (3..=11).contains(&x) && (2..=6).contains(&y),
        ItemIcon::Shovel => (5..=9).contains(&x) && (2..=6).contains(&y) || (x == 7 && y == 7),
        ItemIcon::Axe => {
            (3..=7).contains(&x) && (2..=6).contains(&y) || (x == 8 && (3..=5).contains(&y))
        }
        ItemIcon::Drill => {
            (4..=11).contains(&x) && (3..=6).contains(&y) || (x == 12 && (4..=5).contains(&y))
        }
        _ => false,
    };
    let body = match icon {
        ItemIcon::Food => (x as i32 - 7).pow(2) + (y as i32 - 8).pow(2) <= 25,
        ItemIcon::Bottle => {
            (6..=9).contains(&x) && (2..=5).contains(&y)
                || (4..=11).contains(&x) && (5..=12).contains(&y)
        }
        ItemIcon::Sapling => {
            let row = y / 3;
            (2..=10).contains(&y) && x.abs_diff(7) <= row
        }
        ItemIcon::Dye => {
            (5..=10).contains(&x) && (5..=12).contains(&y) || (6..=9).contains(&x) && y == 4
        }
        ItemIcon::Material => (3..=12).contains(&x) && (4..=12).contains(&y),
        _ => tool_head,
    };
    let stem = match icon {
        ItemIcon::Food => x == 7 && (1..=3).contains(&y),
        ItemIcon::Sapling => x.abs_diff(7) <= 1 && (10..=14).contains(&y),
        ItemIcon::Dye => (6..=9).contains(&x) && (2..=3).contains(&y),
        ItemIcon::Bottle => (6..=9).contains(&x) && y == 1,
        ItemIcon::Material => (3..=12).contains(&x) && (y == 7 || y == 10),
        _ => handle && !tool_head,
    };
    if stem {
        return tone([132, 96, 64], if x == 7 { 12 } else { -9 });
    }
    if body {
        let grain = (texture_noise(x, y, tile.seed) % 9) as i32 - 4;
        return tone(
            tile.color.map(i32::from),
            grain + if x < 7 { 12 } else { -10 },
        );
    }
    [0, 0, 0, 0]
}

fn texture_noise(x: u32, y: u32, salt: u32) -> u32 {
    let mut bits = x.wrapping_mul(0x9E37_79B9) ^ y.wrapping_mul(0x85EB_CA6B) ^ salt;
    bits ^= bits >> 16;
    bits = bits.wrapping_mul(0x7FEB_352D);
    bits ^= bits >> 15;
    bits = bits.wrapping_mul(0x846C_A68B);
    bits ^ (bits >> 16)
}

fn tone(rgb: [i32; 3], value: i32) -> [u8; 4] {
    [
        (rgb[0] + value).clamp(0, 255) as u8,
        (rgb[1] + value).clamp(0, 255) as u8,
        (rgb[2] + value).clamp(0, 255) as u8,
        255,
    ]
}

fn tile_pixel(tile: Tile, x: u32, y: u32) -> [u8; 4] {
    let salt = 7919 * (tile as u32 + 1);
    let grain = (texture_noise(x, y, salt) % 13) as i32 - 6;
    let patch = (texture_noise(x / 3, y / 2, salt) % 21) as i32 - 10;
    let speck = texture_noise(x, y, salt + 139) % 43;
    match tile {
        Tile::Grass => {
            let blade = (x + y / 2) % 4 == 1 && speck < 21;
            let color = if blade { [107, 149, 76] } else { [77, 126, 72] };
            tone(color, grain + patch)
        }
        Tile::GrassSide => {
            let fringe = 3 + texture_noise(x / 2, 1, 421) % 3;
            if y + 1 == fringe {
                tone([58, 95, 54], grain + patch / 2)
            } else if y < fringe {
                tone([80, 129, 70], grain + patch + 5 * i32::from(y == 0))
            } else {
                let dirt = tile_pixel(Tile::Dirt, x, y);
                tone([dirt[0] as i32, dirt[1] as i32, dirt[2] as i32], -3)
            }
        }
        Tile::Dirt => {
            let color = match speck {
                0..=2 => [94, 66, 51],
                3..=5 => [151, 112, 78],
                _ => [126, 90, 64],
            };
            tone(color, grain + patch / 2)
        }
        Tile::Stone => {
            let stratum = (y + (x + 1) / 3 + (x + 1) / 6) % 8;
            let color = match stratum {
                0 if speck > 8 => [107, 118, 123],
                1 if speck > 16 => [143, 153, 154],
                _ => [124, 136, 142],
            };
            tone(color, grain + patch / 2)
        }
        Tile::Sand => {
            let color = match speck {
                0..=2 => [182, 148, 93],
                3..=5 => [231, 205, 152],
                _ => [211, 179, 122],
            };
            tone(color, grain / 2 + patch / 3)
        }
        Tile::Bark => {
            let channel = (x + (y + 2) / 5) % 5;
            let color = match channel {
                0 => [70, 50, 46],
                1 => [121, 87, 61],
                _ => [101, 72, 52],
            };
            tone(color, grain + patch / 3)
        }
        Tile::Rings => {
            let dx = x.abs_diff(8);
            let dy = y.abs_diff(7);
            let distance = dx.max(dy) + dx.min(dy) / 3;
            let color = match distance {
                0 => [116, 76, 44],
                2 | 5 => [151, 111, 67],
                7.. => [85, 61, 48],
                _ => [178, 138, 84],
            };
            tone(color, grain)
        }
        Tile::Pine => {
            let row = y % 4;
            let needle = (x + if x / 4 % 2 == 0 { row } else { 4 - row }) % 4;
            let color = match needle {
                0 => [69, 117, 76],
                1 => [51, 99, 68],
                3 => [36, 76, 58],
                _ => [49, 91, 63],
            };
            tone(color, grain + patch)
        }
        Tile::Slate => {
            let seam = (y + x / 4) % 5;
            let color = if seam == 0 {
                [49, 63, 74]
            } else {
                [67, 81, 93]
            };
            tone(color, grain / 2 + patch / 2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::STONE;
    use glam::Vec3;

    #[test]
    fn generated_item_icons_have_transparent_margins_and_visible_pixels() {
        let textures = generate_textures();
        let tile_bytes = (textures.size * textures.size * 4) as usize;
        assert_eq!(textures.rgba.len(), tile_bytes * textures.layers as usize);
        for item in crate::items::item_defs() {
            let start = item.layer as usize * tile_bytes;
            let pixels = &textures.rgba[start..start + tile_bytes];
            assert!(
                pixels.chunks_exact(4).any(|pixel| pixel[3] == 0),
                "{}",
                item.name
            );
            assert!(
                pixels.chunks_exact(4).any(|pixel| pixel[3] == 255),
                "{}",
                item.name
            );
        }
    }

    #[test]
    fn greedy_faces_repeat_tiles_and_use_outward_winding() {
        let mut world = World::new(99);
        let y = WORLD_HEIGHT - 2;
        for z in 2..4 {
            for x in 2..4 {
                assert!(world.set_block(IVec3::new(x, y, z), STONE));
            }
        }
        let mesh = build_chunk(&world, ChunkPos { x: 0, z: 0 });
        let platform_tops = mesh.vertices.chunks_exact(4).filter(|quad| {
            quad[0].position[1] == (y + 1) as f32 && quad[0].normal == [0.0, 1.0, 0.0]
        });
        let tops: Vec<_> = platform_tops.collect();
        assert_eq!(tops.len(), 1, "a flat 2×2 top should merge into one quad");
        assert_eq!(tops[0][1].uv, [2.0, 2.0]);
        for triangle in mesh.indices.chunks_exact(3) {
            let [a, b, c] =
                [triangle[0], triangle[1], triangle[2]].map(|index| mesh.vertices[index as usize]);
            let [pa, pb, pc] = [a, b, c].map(|vertex| Vec3::from_array(vertex.position));
            assert!((pb - pa).cross(pc - pa).dot(Vec3::from_array(a.normal)) > 0.0);
        }
    }

    #[test]
    fn adjacent_chunks_hide_shared_block_faces_even_when_unloaded() {
        let mut world = World::new(88);
        let y = WORLD_HEIGHT - 2;
        assert!(world.set_block(IVec3::new(-1, y, 2), STONE));
        assert!(world.set_block(IVec3::new(0, y, 2), STONE));
        let mesh = build_chunk(&world, ChunkPos { x: 0, z: 0 });
        assert!(!mesh.vertices.iter().any(|vertex| {
            vertex.position[0] == 0.0
                && vertex.position[1] >= y as f32
                && vertex.normal == [-1.0, 0.0, 0.0]
        }));
    }
}
