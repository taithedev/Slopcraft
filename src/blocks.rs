use std::{collections::HashMap, path::Path, sync::OnceLock};

use crate::{content, items::ItemDef};

pub type BlockId = u16;

// IDs 0 through 7 preserve the original Wildreach save format.
pub const AIR: BlockId = 0;
pub const GRASS: BlockId = 1;
pub const DIRT: BlockId = 2;
pub const STONE: BlockId = 3;
pub const SAND: BlockId = 4;
pub const LOG: BlockId = 5;
pub const LEAVES: BlockId = 6;
pub const SLATE: BlockId = 7;
pub const LIMESTONE: BlockId = 83;
pub const SNOW: BlockId = 284;
pub const RED_SAND: BlockId = 285;
pub const TERRACOTTA: BlockId = 286;
pub const HOTBAR: [BlockId; 7] = [GRASS, DIRT, STONE, SAND, LOG, LEAVES, SLATE];
pub const MAX_TEXTURE_LAYERS: usize = 512;

#[derive(Clone, Copy)]
#[repr(u32)]
pub(crate) enum Tile {
    Grass,
    GrassSide,
    Dirt,
    Stone,
    Sand,
    Bark,
    Rings,
    Pine,
    Slate,
}

impl Tile {
    pub const ALL: [Self; 9] = [
        Self::Grass,
        Self::GrassSide,
        Self::Dirt,
        Self::Stone,
        Self::Sand,
        Self::Bark,
        Self::Rings,
        Self::Pine,
        Self::Slate,
    ];
}

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    Solid,
    Stone,
    Cobble,
    Bricks,
    Tiles,
    Carved,
    Planks,
    Parquet,
    Mosaic,
    Bark,
    Rings,
    Leaves,
    Sand,
    Dirt,
    Fabric,
    Ceramic,
    Metal,
    Ore,
    Marble,
    Crystal,
    Glass,
}

pub(crate) struct TextureSpec {
    pub color: [u8; 3],
    pub pattern: Pattern,
    pub seed: u32,
    pub legacy: Option<Tile>,
    pub icon: Option<ItemIcon>,
}

#[derive(Clone, Copy)]
pub(crate) enum ItemIcon {
    Pickaxe,
    Hammer,
    Shovel,
    Axe,
    Drill,
    Food,
    Bottle,
    Sapling,
    Dye,
    Material,
}

pub struct BlockDef {
    pub id: BlockId,
    pub key: String,
    pub name: String,
    pub category: String,
    pub solid: bool,
    pub opaque: bool,
    pub top: u32,
    pub(crate) bottom: u32,
    pub(crate) side: u32,
}

impl BlockDef {
    pub fn face_layer(&self, normal_y: i32) -> u32 {
        match normal_y {
            1 => self.top,
            -1 => self.bottom,
            _ => self.side,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ContentReport {
    pub builtin_blocks: usize,
    pub mod_blocks: usize,
    pub packs: usize,
    pub items: usize,
}

pub(crate) struct Registry {
    pub blocks: Vec<Option<BlockDef>>,
    pub keys: HashMap<String, BlockId>,
    pub catalog: Vec<BlockId>,
    pub textures: Vec<TextureSpec>,
    pub items: Vec<ItemDef>,
    pub report: ContentReport,
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

pub fn initialize(mods_dir: &Path) -> Result<ContentReport, String> {
    let registry = content::build_registry(mods_dir)?;
    let report = registry.report.clone();
    REGISTRY
        .set(registry)
        .map_err(|_| "The block registry was already initialized".to_owned())?;
    Ok(report)
}

pub(crate) fn registry() -> &'static Registry {
    REGISTRY.get_or_init(content::builtin_registry)
}

#[inline]
pub fn block_def(id: BlockId) -> &'static BlockDef {
    registry()
        .blocks
        .get(usize::from(id))
        .and_then(Option::as_ref)
        .unwrap_or_else(|| registry().blocks[0].as_ref().expect("air is registered"))
}

pub fn is_valid(id: BlockId) -> bool {
    registry()
        .blocks
        .get(usize::from(id))
        .is_some_and(Option::is_some)
}

pub fn block_key(id: BlockId) -> Option<&'static str> {
    registry()
        .blocks
        .get(usize::from(id))
        .and_then(Option::as_ref)
        .map(|block| block.key.as_str())
}

pub fn block_id(key: &str) -> Option<BlockId> {
    registry().keys.get(key).copied()
}

pub fn catalog() -> &'static [BlockId] {
    &registry().catalog
}

pub fn tree_blocks(species: usize) -> Option<(BlockId, BlockId)> {
    let species = content::TREE_SPECIES.get(species)?.0;
    if species == "pine" {
        return Some((LOG, LEAVES));
    }
    Some((
        block_id(&format!("slopcraft:{species}_log"))?,
        block_id(&format!("slopcraft:{species}_leaves"))?,
    ))
}
