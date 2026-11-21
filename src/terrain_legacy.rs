// Generator version 1 is frozen to preserve existing Wildreach worlds.
use super::{hash, slope, smooth_noise};
use crate::blocks::{AIR, BlockId, DIRT, GRASS, LEAVES, LOG, SAND, SLATE, STONE};
use glam::IVec3;
const TREE_SPACING: i32 = 8;
pub(super) const CROWN_RADIUS: i32 = 2;
const MAX_SURFACE: i32 = 67;
const MAX_TREE_HEIGHT: i32 = 7;
pub(super) struct Terrain {
    pub(super) seed: u64,
}
impl Terrain {
    pub(super) fn column(&self, x: i32, z: i32) -> Column {
        let wide = smooth_noise(self.seed, x, z, 72, 1);
        let hills = smooth_noise(self.seed, x, z, 31, 2);
        let roughness = smooth_noise(self.seed, x, z, 11, 3);
        let ridge = 1.0 - smooth_noise(self.seed, x, z, 42, 4).abs();
        let escarpment = slope(x, z, 30, -56, 42.0, 35.0);
        let starting_ridge = slope(x, z, -3, 7, 30.0, 21.0);
        let surface = (26.0
            + wide * 9.0
            + hills * 5.0
            + roughness * 2.2
            + starting_ridge * 15.5
            + escarpment * (21.0 + (ridge * 9.0 + roughness * 7.0)))
            .round()
            .clamp(8.0, MAX_SURFACE as f32) as i32;

        let sand = smooth_noise(self.seed, x, z, 16, 5);
        let stone = smooth_noise(self.seed, x, z, 10, 6);
        let cap = if surface >= 47 && ridge + stone * 0.4 > 0.72 {
            SLATE
        } else if surface >= 45 || (escarpment > 0.35 && stone > 0.18) {
            STONE
        } else if surface <= 20 || (surface <= 25 && sand > 0.22) {
            SAND
        } else {
            GRASS
        };

        Column {
            surface,
            cap,
            slate_depth: 4 + ((roughness + 1.0) * 2.5) as i32,
        }
    }

    pub(super) fn tree(&self, grid_x: i32, grid_z: i32) -> Option<Tree> {
        let bits = hash(self.seed, grid_x, grid_z, 7);
        let x = grid_x * TREE_SPACING + 1 + ((bits & 7) % 6) as i32;
        let z = grid_z * TREE_SPACING + 1 + (((bits >> 3) & 7) % 6) as i32;
        if x.abs().max(z.abs()) <= 8 {
            return None;
        }
        let grove = smooth_noise(self.seed, x, z, 44, 8);
        let chance = (bits >> 25) as u8;
        if f32::from(chance) > 82.0 + grove * 56.0 {
            return None;
        }
        let column = self.column(x, z);
        if column.cap != GRASS {
            return None;
        }
        Some(Tree {
            base: IVec3::new(x, column.surface + 1, z),
            height: MAX_TREE_HEIGHT - 3 + ((bits >> 10) & 3) as i32,
        })
    }
}

pub(super) struct Column {
    pub(super) surface: i32,
    cap: BlockId,
    slate_depth: i32,
}

impl Column {
    pub(super) fn block(&self, y: i32) -> BlockId {
        if y <= self.slate_depth {
            return SLATE;
        }

        match self.surface - y {
            0 => self.cap,
            1..=3 if self.cap == GRASS => DIRT,
            1..=3 if self.cap == SAND => SAND,
            1..=2 if self.cap == SLATE => SLATE,
            _ => STONE,
        }
    }
}

pub(super) struct Tree {
    pub(super) base: IVec3,
    pub(super) height: i32,
}

impl Tree {
    pub(super) fn block(&self, pos: IVec3) -> BlockId {
        let offset = pos - self.base;
        if !(0..=self.height).contains(&offset.y) {
            return AIR;
        }
        let distance = offset.x.abs() + offset.z.abs();
        if distance == 0 {
            return if offset.y < self.height { LOG } else { LEAVES };
        }
        let layer = self.height - offset.y;
        if (layer == 1 && distance == 1)
            || ((2..=3).contains(&layer)
                && distance <= 3
                && offset.x.abs().max(offset.z.abs()) <= CROWN_RADIUS)
            || (layer == 4 && self.height >= 6 && distance <= 2)
        {
            LEAVES
        } else {
            AIR
        }
    }
}
