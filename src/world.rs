use crate::blocks::{
    AIR, BlockId, DIRT, GRASS, LEAVES, LIMESTONE, LOG, RED_SAND, SAND, SLATE, SNOW, STONE,
    TERRACOTTA, block_def, is_valid, tree_blocks,
};
use glam::{IVec3, Vec3};
use std::collections::HashMap;
use std::path::Path;

#[path = "terrain_legacy.rs"]
mod legacy;
#[path = "save.rs"]
mod save;

pub const CHUNK_SIZE: i32 = 16;
pub const WORLD_HEIGHT: i32 = 128;
const CHUNK_VOLUME: usize = (CHUNK_SIZE * CHUNK_SIZE * WORLD_HEIGHT) as usize;
const TREE_SPACING: i32 = 11;
const CROWN_RADIUS: i32 = 4;
const HORIZONTAL_LIMIT: i32 = 1_048_575;
const GENERATOR_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub fn from_block(x: i32, z: i32) -> Self {
        Self {
            x: x.div_euclid(CHUNK_SIZE),
            z: z.div_euclid(CHUNK_SIZE),
        }
    }
    fn origin(self) -> IVec3 {
        IVec3::new(self.x * CHUNK_SIZE, 0, self.z * CHUNK_SIZE)
    }
}

pub struct World {
    seed: u64,
    generator_version: u32,
    player_pose: Option<([f32; 3], f32, f32)>,
    chunks: HashMap<ChunkPos, Vec<BlockId>>,
    edits: HashMap<ChunkPos, HashMap<usize, BlockId>>,
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            generator_version: GENERATOR_VERSION,
            player_pose: None,
            chunks: HashMap::new(),
            edits: HashMap::new(),
        }
    }
    pub fn seed(&self) -> u64 {
        self.seed
    }
    pub fn generator_version(&self) -> u32 {
        self.generator_version
    }
    pub fn player_pose(&self) -> Option<([f32; 3], f32, f32)> {
        self.player_pose
    }
    pub fn set_player_pose(&mut self, feet: [f32; 3], yaw: f32, pitch: f32) -> bool {
        if !valid_pose(feet, yaw, pitch) {
            return false;
        }
        self.player_pose = Some((feet, yaw, pitch));
        true
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        save::load(path)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        save::write(self, path)
    }
    pub fn change_count(&self) -> usize {
        self.edits.values().map(HashMap::len).sum()
    }
    pub fn biome_name(&self, x: i32, z: i32) -> &'static str {
        if self.generator_version == 1 {
            return "Wildreach Highlands";
        }
        self.column(x, z).biome.name()
    }

    pub fn spawn(&self) -> Vec3 {
        for radius in 0_i32..=6 {
            for z in -radius..=radius {
                for x in -radius..=radius {
                    if x.abs().max(z.abs()) != radius {
                        continue;
                    }
                    let surface = self.surface(x, z);
                    let floor = self.skyline(x, z);
                    if (surface - 5..=surface + 4).contains(&floor) {
                        return Vec3::new(x as f32 + 0.5, floor as f32 + 1.01, z as f32 + 0.5);
                    }
                }
            }
        }
        Vec3::new(0.5, self.skyline(0, 0) as f32 + 1.01, 0.5)
    }
    fn skyline(&self, x: i32, z: i32) -> i32 {
        (0..WORLD_HEIGHT)
            .rev()
            .find(|&y| block_def(self.block(IVec3::new(x, y, z))).solid)
            .unwrap_or(0)
    }
    fn surface(&self, x: i32, z: i32) -> i32 {
        if self.generator_version == 1 {
            legacy::Terrain { seed: self.seed }.column(x, z).surface
        } else {
            self.column(x, z).surface
        }
    }

    pub fn load_chunk(&mut self, pos: ChunkPos) {
        if self.chunks.contains_key(&pos) {
            return;
        }
        let origin = pos.origin();
        let mut blocks = vec![AIR; CHUNK_VOLUME];
        if self.generator_version == 1 {
            self.fill_legacy(origin, &mut blocks);
        } else {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let column = self.column(origin.x + x, origin.z + z);
                    for y in 0..=column.surface {
                        blocks[index(x, y, z)] = column.block(y);
                    }
                }
            }
            let min = origin - IVec3::new(CROWN_RADIUS, 0, CROWN_RADIUS);
            let max = origin
                + IVec3::new(
                    CHUNK_SIZE - 1 + CROWN_RADIUS,
                    0,
                    CHUNK_SIZE - 1 + CROWN_RADIUS,
                );
            for grid_z in min.z.div_euclid(TREE_SPACING)..=max.z.div_euclid(TREE_SPACING) {
                for grid_x in min.x.div_euclid(TREE_SPACING)..=max.x.div_euclid(TREE_SPACING) {
                    if let Some(tree) = self.tree(grid_x, grid_z) {
                        self.write_tree(origin, &mut blocks, tree);
                    }
                }
            }
        }
        if let Some(edits) = self.edits.get(&pos) {
            for (&index, &block) in edits {
                blocks[index] = block;
            }
        }
        self.chunks.insert(pos, blocks);
    }

    fn fill_legacy(&self, origin: IVec3, blocks: &mut [BlockId]) {
        let terrain = legacy::Terrain { seed: self.seed };
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let column = terrain.column(origin.x + x, origin.z + z);
                for y in 0..=column.surface {
                    blocks[index(x, y, z)] = column.block(y);
                }
            }
        }
        let min = origin - IVec3::new(2, 0, 2);
        let max = origin + IVec3::new(CHUNK_SIZE + 1, 0, CHUNK_SIZE + 1);
        for grid_z in min.z.div_euclid(8)..=max.z.div_euclid(8) {
            for grid_x in min.x.div_euclid(8)..=max.x.div_euclid(8) {
                let Some(tree) = terrain.tree(grid_x, grid_z) else {
                    continue;
                };
                let base = tree.base - origin;
                for z in (base.z - 2).max(0)..=(base.z + 2).min(CHUNK_SIZE - 1) {
                    for x in (base.x - 2).max(0)..=(base.x + 2).min(CHUNK_SIZE - 1) {
                        for y in base.y..=base.y + tree.height {
                            let block = tree.block(origin + IVec3::new(x, y, z));
                            let existing = &mut blocks[index(x, y, z)];
                            if *existing == AIR || (*existing == LEAVES && block == LOG) {
                                *existing = block;
                            }
                        }
                    }
                }
            }
        }
    }

    fn write_tree(&self, origin: IVec3, blocks: &mut [BlockId], tree: Tree) {
        let base = tree.base - origin;
        for z in (base.z - CROWN_RADIUS).max(0)..=(base.z + CROWN_RADIUS).min(CHUNK_SIZE - 1) {
            for x in (base.x - CROWN_RADIUS).max(0)..=(base.x + CROWN_RADIUS).min(CHUNK_SIZE - 1) {
                let surface = self.column(origin.x + x, origin.z + z).surface;
                for y in base.y.max(surface + 1)..=(base.y + tree.height).min(WORLD_HEIGHT - 1) {
                    let block = tree.block(origin + IVec3::new(x, y, z));
                    let existing = &mut blocks[index(x, y, z)];
                    if *existing == AIR {
                        *existing = block;
                    }
                }
            }
        }
    }
    pub fn unload_chunk(&mut self, pos: ChunkPos) {
        self.chunks.remove(&pos);
    }
    pub fn block(&self, pos: IVec3) -> BlockId {
        if pos.y <= 0 {
            return SLATE;
        }
        if pos.y >= WORLD_HEIGHT {
            return AIR;
        }
        let chunk = ChunkPos::from_block(pos.x, pos.z);
        let index = index_in_chunk(pos);
        if let Some(blocks) = self.chunks.get(&chunk) {
            return blocks[index];
        }
        if let Some(&block) = self.edits.get(&chunk).and_then(|edits| edits.get(&index)) {
            return block;
        }
        self.generated_block(pos)
    }
    pub fn set_block(&mut self, pos: IVec3, id: BlockId) -> bool {
        if !editable(pos) || !is_valid(id) || self.block(pos) == id {
            return false;
        }
        let chunk = ChunkPos::from_block(pos.x, pos.z);
        let index = index_in_chunk(pos);
        if self.generated_block(pos) == id {
            if let Some(edits) = self.edits.get_mut(&chunk) {
                edits.remove(&index);
                if edits.is_empty() {
                    self.edits.remove(&chunk);
                }
            }
        } else {
            self.edits.entry(chunk).or_default().insert(index, id);
        }
        if let Some(blocks) = self.chunks.get_mut(&chunk) {
            blocks[index] = id;
        }
        true
    }

    pub fn plant_tree(&mut self, base: IVec3, species: usize) -> Vec<IVec3> {
        let Some((log, leaves)) = tree_blocks(species) else {
            return Vec::new();
        };
        let soil = self.block(base - IVec3::Y);
        if ![GRASS, DIRT, SAND, RED_SAND].contains(&soil) {
            return Vec::new();
        }
        let tree = Tree {
            base,
            species,
            height: tree_height(species, 1),
            log,
            leaves,
        };
        let mut cells = Vec::new();
        for z in -CROWN_RADIUS..=CROWN_RADIUS {
            for x in -CROWN_RADIUS..=CROWN_RADIUS {
                for y in 0..=tree.height {
                    let position = base + IVec3::new(x, y, z);
                    let block = tree.block(position);
                    if block == AIR {
                        continue;
                    }
                    // Check the whole crown before changing the world.
                    if !editable(position) || self.block(position) != AIR {
                        return Vec::new();
                    }
                    cells.push((position, block));
                }
            }
        }
        for &(position, block) in &cells {
            self.set_block(position, block);
        }
        cells.into_iter().map(|(position, _)| position).collect()
    }

    fn generated_block(&self, pos: IVec3) -> BlockId {
        if self.generator_version == 1 {
            return self.legacy_block(pos);
        }
        if pos.y >= 124 {
            return AIR;
        }
        let column = self.column(pos.x, pos.z);
        if pos.y <= column.surface {
            return column.block(pos.y);
        }
        let min_x = (pos.x - CROWN_RADIUS).div_euclid(TREE_SPACING);
        let max_x = (pos.x + CROWN_RADIUS).div_euclid(TREE_SPACING);
        let min_z = (pos.z - CROWN_RADIUS).div_euclid(TREE_SPACING);
        let max_z = (pos.z + CROWN_RADIUS).div_euclid(TREE_SPACING);
        // Keep the same order as chunk generation for overlapping crowns.
        for grid_z in min_z..=max_z {
            for grid_x in min_x..=max_x {
                if let Some(tree) = self.tree(grid_x, grid_z) {
                    let block = tree.block(pos);
                    if block != AIR {
                        return block;
                    }
                }
            }
        }
        AIR
    }
    fn legacy_block(&self, pos: IVec3) -> BlockId {
        if pos.y > 75 {
            return AIR;
        }
        let terrain = legacy::Terrain { seed: self.seed };
        let column = terrain.column(pos.x, pos.z);
        if pos.y <= column.surface {
            return column.block(pos.y);
        }
        let mut leaves = false;
        for grid_z in (pos.z - 2).div_euclid(8)..=(pos.z + 2).div_euclid(8) {
            for grid_x in (pos.x - 2).div_euclid(8)..=(pos.x + 2).div_euclid(8) {
                if let Some(tree) = terrain.tree(grid_x, grid_z) {
                    match tree.block(pos) {
                        LOG => return LOG,
                        LEAVES => leaves = true,
                        _ => (),
                    }
                }
            }
        }
        if leaves { LEAVES } else { AIR }
    }

    fn column(&self, x: i32, z: i32) -> Column {
        let broad = smooth_noise(self.seed, x, z, 128, 101);
        let hills = smooth_noise(self.seed, x, z, 37, 102);
        let detail = smooth_noise(self.seed, x, z, 13, 103);
        let spine = 1.0 - smooth_noise(self.seed, x + 21, z - 37, 59, 104).abs();
        let mountain = ((smooth_noise(self.seed, x, z, 157, 105) + 0.15) * 1.8).clamp(0.0, 1.0);
        let landmark = slope(x, z, 45, -93, 70.0, 62.0);
        let home = slope(x, z, -7, 6, 35.0, 28.0);
        let valley = smooth_noise(self.seed, x, z, 67, 106).abs();
        let valley_depth = (1.0 - valley * 4.5).max(0.0);
        let surface = (44.0
            + broad * 12.0
            + hills * 7.0
            + detail * 2.0
            + mountain * (1.0 - home * 0.72) * (spine * spine * 36.0 + detail * 6.0)
            + landmark * (48.0 + spine * 9.0)
            + home * 9.0
            - valley_depth * 13.0)
            .round()
            .clamp(15.0, 110.0) as i32;
        let moisture = (smooth_noise(self.seed, x, z, 91, 107) - (x as f32 / 230.0).tanh() * 0.52)
            * (1.0 - home);
        let heat = (smooth_noise(self.seed, x, z, 173, 108) + (x as f32 / 200.0).tanh() * 0.4)
            * (1.0 - home);
        let biome = if surface >= 80 {
            Biome::Alpine
        } else if moisture < -0.28 && heat > 0.0 {
            Biome::Badlands
        } else if moisture < -0.19 {
            Biome::Dunes
        } else if moisture > 0.14 && heat > -0.05 {
            Biome::Woodland
        } else if moisture > 0.04 || heat < -0.3 {
            Biome::Pine
        } else {
            Biome::Meadow
        };
        let rock = surface > 69 && (detail > -0.3 || mountain > 0.7);
        let cap = match biome {
            Biome::Alpine if surface > 89 || detail > -0.18 => SNOW,
            Biome::Alpine => SLATE,
            Biome::Badlands => RED_SAND,
            Biome::Dunes => SAND,
            _ if rock => STONE,
            _ => GRASS,
        };
        let substrate = match biome {
            Biome::Badlands => TERRACOTTA,
            Biome::Dunes => SAND,
            _ if cap == GRASS => DIRT,
            _ => STONE,
        };
        let cave_lane = smooth_noise(self.seed, x, z, 26, 110).abs();
        let cave_center = 17.0
            + smooth_noise(self.seed, x, z, 67, 111) * 9.0
            + smooth_noise(self.seed, x, z, 19, 112) * 3.5;
        let entrance_lane = smooth_noise(self.seed, x, z, 31, 113);
        let no_caves = x.abs().max(z.abs()) < 12;
        Column {
            surface,
            biome,
            cap,
            substrate,
            cave_center,
            cave_radius: if no_caves {
                0.0
            } else {
                (1.0 - cave_lane * 4.2).max(0.0) * 3.5
            },
            upper_cave_center: 37.0 + broad * 10.0 + hills * 7.0,
            upper_cave_radius: if no_caves {
                0.0
            } else {
                ((entrance_lane - 0.38) * 12.0).clamp(0.0, 5.5)
            },
        }
    }
    fn tree(&self, grid_x: i32, grid_z: i32) -> Option<Tree> {
        let bits = hash(self.seed, grid_x, grid_z, 115);
        let x = grid_x * TREE_SPACING + 2 + (bits % 7) as i32;
        let z = grid_z * TREE_SPACING + 2 + ((bits >> 3) % 7) as i32;
        if x.abs().max(z.abs()) <= 10 {
            return None;
        }
        let column = self.column(x, z);
        if column.block(column.surface) != GRASS {
            return None;
        }
        let chance = (bits >> 25) as u8;
        let species = match column.biome {
            Biome::Woodland if chance < 200 => [1, 2, 3, 4, 6, 8, 9][((bits >> 12) % 7) as usize],
            Biome::Pine if chance < 166 => [0, 5, 11][((bits >> 12) % 3) as usize],
            Biome::Meadow if chance < 31 => [1, 2, 7, 10][((bits >> 12) % 4) as usize],
            _ => return None,
        };
        let (log, leaves) = tree_blocks(species)?;
        Some(Tree {
            base: IVec3::new(x, column.surface + 1, z),
            species,
            height: tree_height(species, bits >> 35),
            log,
            leaves,
        })
    }
}

#[derive(Clone, Copy)]
enum Biome {
    Meadow,
    Dunes,
    Badlands,
    Alpine,
    Woodland,
    Pine,
}
impl Biome {
    fn name(self) -> &'static str {
        match self {
            Self::Meadow => "Meadow",
            Self::Dunes => "Dunes",
            Self::Badlands => "Badlands",
            Self::Alpine => "Alpine Highlands",
            Self::Woodland => "Broadleaf Woods",
            Self::Pine => "Pine Forest",
        }
    }
}

struct Column {
    surface: i32,
    biome: Biome,
    cap: BlockId,
    substrate: BlockId,
    cave_center: f32,
    cave_radius: f32,
    upper_cave_center: f32,
    upper_cave_radius: f32,
}
impl Column {
    fn block(&self, y: i32) -> BlockId {
        if y <= 3 {
            return SLATE;
        }
        if (y as f32 - self.cave_center).abs() < self.cave_radius
            || (y as f32 - self.upper_cave_center).abs() < self.upper_cave_radius
        {
            return AIR;
        }
        match self.surface - y {
            0 => self.cap,
            1..=3 => self.substrate,
            _ if matches!(self.biome, Biome::Badlands) && y > self.surface - 20 => {
                if y.rem_euclid(7) <= 1 {
                    LIMESTONE
                } else {
                    TERRACOTTA
                }
            }
            _ if y > 22 && y.rem_euclid(19) <= 2 => LIMESTONE,
            _ => STONE,
        }
    }
}

struct Tree {
    base: IVec3,
    height: i32,
    species: usize,
    log: BlockId,
    leaves: BlockId,
}
impl Tree {
    fn block(&self, pos: IVec3) -> BlockId {
        let p = pos - self.base;
        if !(0..=self.height).contains(&p.y) {
            return AIR;
        }
        let radial = p.x * p.x + p.z * p.z;
        let tier = self.height - p.y;
        let conifer = matches!(self.species, 0 | 5 | 11);
        let trunk = if conifer {
            p.y < self.height - 1
        } else {
            p.y <= self.height - 2
        };
        if p.x == 0 && p.z == 0 && trunk {
            return self.log;
        }
        let foliage = match self.species {
            0 | 5 | 11 => {
                let radius = ((tier + 1) / 3).clamp(0, 3);
                p.y >= 3
                    && radial <= radius * radius + i32::from(tier > 0)
                    && !(tier % 3 == 2 && radial > (radius - 1).max(0).pow(2) + 1)
            }
            7 => {
                (tier <= 2 && radial <= 12 && p.x.abs() <= 4 && p.z.abs() <= 3)
                    || (tier == 3 && radial <= 5)
            }
            2 | 10 => tier <= 5 && radial + (tier - 2).pow(2) <= 10,
            6 => {
                (tier <= 4 && radial + (tier - 2).pow(2) <= 17)
                    || ((3..=6).contains(&tier)
                        && (8..=13).contains(&radial)
                        && (p.x + p.z).rem_euclid(3) != 0)
            }
            _ => tier <= 5 && radial + (tier - 2).pow(2) <= 14,
        };
        if foliage { self.leaves } else { AIR }
    }
}
fn tree_height(species: usize, bits: u64) -> i32 {
    let variation = (bits % 3) as i32;
    match species {
        0 | 5 => 10 + variation,
        11 => 13 + variation,
        7 => 6 + variation,
        _ => 7 + variation,
    }
}
fn valid_pose(feet: [f32; 3], yaw: f32, pitch: f32) -> bool {
    feet.iter().all(|v| v.is_finite())
        && yaw.is_finite()
        && pitch.is_finite()
        && feet[0].abs() <= HORIZONTAL_LIMIT as f32
        && feet[2].abs() <= HORIZONTAL_LIMIT as f32
        && (1.0..=WORLD_HEIGHT as f32 + 2.0).contains(&feet[1])
        && pitch.abs() <= std::f32::consts::FRAC_PI_2
}
fn editable(pos: IVec3) -> bool {
    (1..WORLD_HEIGHT).contains(&pos.y)
        && (-HORIZONTAL_LIMIT..=HORIZONTAL_LIMIT).contains(&pos.x)
        && (-HORIZONTAL_LIMIT..=HORIZONTAL_LIMIT).contains(&pos.z)
}
fn index(x: i32, y: i32, z: i32) -> usize {
    ((y * CHUNK_SIZE + z) * CHUNK_SIZE + x) as usize
}
fn index_in_chunk(pos: IVec3) -> usize {
    index(
        pos.x.rem_euclid(CHUNK_SIZE),
        pos.y,
        pos.z.rem_euclid(CHUNK_SIZE),
    )
}
fn position_of(chunk: ChunkPos, index: usize) -> IVec3 {
    let index = index as i32;
    chunk.origin()
        + IVec3::new(
            index % CHUNK_SIZE,
            index / (CHUNK_SIZE * CHUNK_SIZE),
            index / CHUNK_SIZE % CHUNK_SIZE,
        )
}
fn slope(x: i32, z: i32, center_x: i32, center_z: i32, radius_x: f32, radius_z: f32) -> f32 {
    let dx = (x as f32 - center_x as f32) / radius_x;
    let dz = (z as f32 - center_z as f32) / radius_z;
    let t = (1.0 - dx * dx - dz * dz).max(0.0);
    t * t * (3.0 - 2.0 * t)
}
fn smooth_noise(seed: u64, x: i32, z: i32, period: i32, salt: u64) -> f32 {
    let grid_x = x.div_euclid(period);
    let grid_z = z.div_euclid(period);
    let smooth = |offset: i32| {
        let t = offset.rem_euclid(period) as f32 / period as f32;
        t * t * (3.0 - 2.0 * t)
    };
    let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let lattice = |x, z| ((hash(seed, x, z, salt) >> 40) as f32) / 8_388_608.0 - 1.0;
    let tx = smooth(x);
    let tz = smooth(z);
    let near = mix(lattice(grid_x, grid_z), lattice(grid_x + 1, grid_z), tx);
    let far = mix(
        lattice(grid_x, grid_z + 1),
        lattice(grid_x + 1, grid_z + 1),
        tx,
    );
    mix(near, far, tz)
}
fn hash(seed: u64, x: i32, z: i32, salt: u64) -> u64 {
    let mut bits = seed
        ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (z as u64).wrapping_mul(0xD1B5_4A32_D192_ED03)
        ^ salt.wrapping_mul(0x94D0_49BB_1331_11EB);
    bits = (bits ^ (bits >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    bits = (bits ^ (bits >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    bits ^ (bits >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_and_procedural_blocks_agree_at_negative_chunk_boundaries() {
        for generator_version in [1, 2] {
            let mut world = World::new(42);
            world.generator_version = generator_version;
            for chunk in [
                ChunkPos { x: -2, z: -3 },
                ChunkPos { x: -1, z: -3 },
                ChunkPos { x: 3, z: -7 },
            ] {
                world.load_chunk(chunk);
                for z in 0..CHUNK_SIZE {
                    for x in 0..CHUNK_SIZE {
                        for y in 1..WORLD_HEIGHT {
                            let pos = chunk.origin() + IVec3::new(x, y, z);
                            assert_eq!(
                                world.block(pos),
                                world.generated_block(pos),
                                "generator {generator_version} at {pos}"
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(ChunkPos::from_block(-1, -16), ChunkPos { x: -1, z: -1 });
    }
    #[test]
    fn edits_survive_chunk_reload_and_revert_to_natural_terrain() {
        let mut world = World::new(42);
        let pos = IVec3::new(-16, 4, -1);
        let chunk = ChunkPos::from_block(pos.x, pos.z);
        let original = world.block(pos);
        world.load_chunk(chunk);
        assert!(world.set_block(pos, AIR));
        assert!(!world.set_block(pos.with_y(0), AIR));
        assert_eq!(world.change_count(), 1);
        world.unload_chunk(chunk);
        assert_eq!(world.block(pos), AIR);
        world.load_chunk(chunk);
        assert_eq!(world.block(pos), AIR);
        assert!(world.set_block(pos, original));
        assert_eq!(world.change_count(), 0);
    }
    #[test]
    fn planting_checks_the_entire_tree_before_changing_any_blocks() {
        let mut world = World::new(184203);
        let base = world.spawn().floor().as_ivec3();
        world.set_block(base + IVec3::Y * 5, STONE);
        let before = world.change_count();
        assert!(world.plant_tree(base, 1).is_empty());
        assert_eq!(world.change_count(), before);
        world.set_block(base + IVec3::Y * 5, AIR);
        let changes = world.plant_tree(base, 1);
        assert!(changes.len() > 30);
        assert_ne!(world.block(base), AIR);
    }

    #[test]
    fn terrain_contains_six_biomes_open_caves_and_distinct_tree_families() {
        let world = World::new(184203);
        let mut biomes = std::collections::HashSet::new();
        let mut open_caves = 0;
        for z in (-256..=256).step_by(4) {
            for x in (-256..=256).step_by(4) {
                let column = world.column(x, z);
                biomes.insert(column.biome.name());
                open_caves += usize::from(column.block(column.surface) == AIR);
            }
        }
        let mut families = std::collections::HashSet::new();
        for z in -24..=24 {
            for x in -24..=24 {
                if let Some(tree) = world.tree(x, z) {
                    families.insert(match tree.species {
                        0 | 5 | 11 => 0,
                        2 | 10 => 1,
                        6 => 2,
                        7 => 3,
                        _ => 4,
                    });
                }
            }
        }
        assert_eq!(biomes.len(), 6);
        assert!(open_caves > 0);
        assert!(families.len() >= 3);
    }

    #[test]
    fn legacy_terrain_matches_the_original_release_fingerprint() {
        let mut world = World::new(184203);
        world.generator_version = 1;
        let mut fingerprint: u64 = 14_695_981_039_346_656_037;
        for z in -3..=3 {
            for x in -3..=3 {
                world.load_chunk(ChunkPos { x, z });
                for y in 0..80 {
                    for dz in 0..CHUNK_SIZE {
                        for dx in 0..CHUNK_SIZE {
                            let block = world.block(IVec3::new(
                                x * CHUNK_SIZE + dx,
                                y,
                                z * CHUNK_SIZE + dz,
                            ));
                            fingerprint =
                                (fingerprint ^ u64::from(block)).wrapping_mul(1_099_511_628_211);
                        }
                    }
                }
            }
        }
        assert_eq!(fingerprint, 0xb505_1287_51f7_d22f);
    }

    #[test]
    fn benchmark_generation() {
        let start = std::time::Instant::now();
        let mut world = World::new(184203);
        for z in -8..=8 {
            for x in -8..=8 {
                world.load_chunk(ChunkPos { x, z });
            }
        }
        eprintln!(
            "Generated 289 chunks in {:.2?} using {:.1} MiB of voxel data",
            start.elapsed(),
            (CHUNK_VOLUME * 2 * world.chunks.len()) as f32 / 1_048_576.0
        );
    }
}
