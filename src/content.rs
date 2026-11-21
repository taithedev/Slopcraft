use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{
    blocks::{
        BlockDef, BlockId, ContentReport, MAX_TEXTURE_LAYERS, Pattern, Registry, TextureSpec, Tile,
    },
    items,
};

pub(crate) const TREE_SPECIES: [(&str, &str, [u8; 3]); 12] = [
    ("pine", "Pine", [151, 112, 67]),
    ("oak", "Oak", [166, 125, 76]),
    ("birch", "Birch", [206, 189, 148]),
    ("ash", "Ash", [168, 159, 124]),
    ("maple", "Maple", [191, 139, 90]),
    ("cedar", "Cedar", [155, 95, 73]),
    ("willow", "Willow", [132, 148, 92]),
    ("acacia", "Acacia", [184, 100, 63]),
    ("cherry", "Cherry", [181, 108, 116]),
    ("walnut", "Walnut", [107, 79, 67]),
    ("aspen", "Aspen", [197, 179, 112]),
    ("redwood", "Redwood", [141, 73, 59]),
];

const STONES: [(&str, &str, [u8; 3]); 16] = [
    ("granite", "Granite", [160, 130, 123]),
    ("basalt", "Basalt", [76, 85, 91]),
    ("marble", "Marble", [205, 209, 205]),
    ("limestone", "Limestone", [193, 185, 147]),
    ("sandstone", "Sandstone", [202, 170, 116]),
    ("andesite", "Andesite", [132, 138, 139]),
    ("diorite", "Diorite", [177, 182, 176]),
    ("gabbro", "Gabbro", [91, 104, 106]),
    ("quartzite", "Quartzite", [196, 177, 167]),
    ("travertine", "Travertine", [202, 186, 146]),
    ("onyx", "Onyx", [60, 68, 77]),
    ("obsidian", "Obsidian", [65, 56, 87]),
    ("jade", "Jade", [94, 151, 123]),
    ("soapstone", "Soapstone", [122, 150, 148]),
    ("pumice", "Pumice", [161, 158, 151]),
    ("tuff", "Tuff", [143, 148, 116]),
];

const COLORS: [(&str, &str, [u8; 3]); 16] = [
    ("ivory", "Ivory", [224, 219, 200]),
    ("amber", "Amber", [226, 162, 60]),
    ("coral", "Coral", [212, 116, 104]),
    ("scarlet", "Scarlet", [180, 67, 69]),
    ("rose", "Rose", [213, 142, 168]),
    ("orchid", "Orchid", [163, 115, 189]),
    ("violet", "Violet", [117, 87, 165]),
    ("indigo", "Indigo", [82, 98, 162]),
    ("azure", "Azure", [71, 141, 186]),
    ("teal", "Teal", [63, 155, 150]),
    ("mint", "Mint", [123, 187, 151]),
    ("moss", "Moss", [105, 139, 80]),
    ("ochre", "Ochre", [187, 155, 69]),
    ("umber", "Umber", [135, 101, 73]),
    ("silver", "Silver", [162, 172, 177]),
    ("charcoal", "Charcoal", [63, 72, 82]),
];

const METALS: [(&str, &str, [u8; 3]); 8] = [
    ("iron", "Iron", [154, 165, 173]),
    ("copper", "Copper", [184, 117, 80]),
    ("bronze", "Bronze", [168, 130, 67]),
    ("brass", "Brass", [195, 166, 84]),
    ("gold", "Gold", [222, 184, 64]),
    ("cobalt", "Cobalt", [85, 114, 171]),
    ("titanium", "Titanium", [166, 159, 178]),
    ("verdigris", "Verdigris", [83, 150, 135]),
];

const GEMS: [(&str, &str, [u8; 3]); 16] = [
    ("coal", "Coal", [57, 64, 74]),
    ("hematite", "Hematite", [164, 100, 85]),
    ("malachite", "Malachite", [53, 161, 104]),
    ("lapis", "Lapis", [68, 102, 189]),
    ("amethyst", "Amethyst", [162, 112, 194]),
    ("citrine", "Citrine", [216, 180, 89]),
    ("garnet", "Garnet", [160, 55, 83]),
    ("aquamarine", "Aquamarine", [99, 190, 192]),
    ("tourmaline", "Tourmaline", [168, 89, 143]),
    ("peridot", "Peridot", [161, 185, 72]),
    ("moonstone", "Moonstone", [192, 204, 211]),
    ("sunstone", "Sunstone", [216, 136, 75]),
    ("fluorite", "Fluorite", [140, 167, 198]),
    ("pyrite", "Pyrite", [175, 162, 88]),
    ("jasper", "Jasper", [177, 100, 70]),
    ("opal", "Opal", [172, 209, 195]),
];

fn stable_hash(key: &str) -> u32 {
    key.bytes().fold(2_166_136_261, |value, byte| {
        (value ^ u32::from(byte)).wrapping_mul(16_777_619)
    })
}

fn texture(registry: &mut Registry, color: [u8; 3], pattern: Pattern, key: &str) -> u32 {
    let layer = registry.textures.len() as u32;
    registry.textures.push(TextureSpec {
        color,
        pattern,
        seed: stable_hash(key),
        legacy: None,
        icon: None,
    });
    layer
}

fn insert(registry: &mut Registry, block: BlockDef) {
    let id = block.id;
    let needed = usize::from(id) + 1;
    if registry.blocks.len() < needed {
        registry.blocks.resize_with(needed, || None);
    }
    registry.keys.insert(block.key.clone(), id);
    if id != 0 {
        registry.catalog.push(id);
    }
    registry.blocks[usize::from(id)] = Some(block);
}

fn uniform(
    registry: &mut Registry,
    key: String,
    name: String,
    category: &str,
    color: [u8; 3],
    pattern: Pattern,
) {
    let id = registry.catalog.len() as BlockId + 1;
    let key = format!("slopcraft:{key}");
    let layer = texture(registry, color, pattern, &key);
    insert(
        registry,
        BlockDef {
            id,
            key,
            name,
            category: category.to_owned(),
            solid: true,
            opaque: true,
            top: layer,
            bottom: layer,
            side: layer,
        },
    );
}

pub(crate) fn builtin_registry() -> Registry {
    let mut registry = Registry {
        blocks: Vec::new(),
        keys: HashMap::new(),
        catalog: Vec::new(),
        textures: Tile::ALL
            .into_iter()
            .map(|tile| TextureSpec {
                color: [0; 3],
                pattern: Pattern::Solid,
                seed: 0,
                legacy: Some(tile),
                icon: None,
            })
            .collect(),
        items: Vec::new(),
        report: ContentReport {
            builtin_blocks: 300,
            mod_blocks: 0,
            packs: 0,
            items: 75,
        },
    };
    let originals = [
        ("air", "Air", 0, 0, 0),
        ("grass", "Grass", 0, 2, 1),
        ("loam", "Loam", 2, 2, 2),
        ("stone", "Stone", 3, 3, 3),
        ("sand", "Sand", 4, 4, 4),
        ("pine_log", "Pine Log", 6, 6, 5),
        ("pine_leaves", "Pine Needles", 7, 7, 7),
        ("slate", "Slate", 8, 8, 8),
    ];
    for (id, (key, name, top, bottom, side)) in originals.into_iter().enumerate() {
        insert(
            &mut registry,
            BlockDef {
                id: id as BlockId,
                key: format!("slopcraft:{key}"),
                name: name.to_owned(),
                category: "Natural".to_owned(),
                solid: id != 0,
                opaque: id != 0,
                top,
                bottom,
                side,
            },
        );
    }
    for (species, name, color) in TREE_SPECIES {
        let log_key = if species == "pine" {
            "carved_pine_log".to_owned()
        } else {
            format!("{species}_log")
        };
        uniform(
            &mut registry,
            log_key.clone(),
            if species == "pine" {
                "Carved Pine Log".to_owned()
            } else {
                format!("{name} Log")
            },
            "Woods",
            color,
            Pattern::Bark,
        );
        let rings = texture(&mut registry, color, Pattern::Rings, &log_key);
        let log = registry.blocks.last_mut().unwrap().as_mut().unwrap();
        log.top = rings;
        log.bottom = rings;
        for (suffix, label, pattern) in [
            ("planks", "Planks", Pattern::Planks),
            ("parquet", "Parquet", Pattern::Parquet),
            ("mosaic", "Mosaic", Pattern::Mosaic),
        ] {
            uniform(
                &mut registry,
                format!("{species}_{suffix}"),
                format!("{name} {label}"),
                "Woods",
                color,
                pattern,
            );
        }
        let leaves_color = [
            color[0] / 2 + 15,
            (u16::from(color[1]) / 2 + 65).min(210) as u8,
            color[2] / 2 + 20,
        ];
        uniform(
            &mut registry,
            if species == "pine" {
                "pine_fronds".to_owned()
            } else {
                format!("{species}_leaves")
            },
            format!(
                "{name} {}",
                if species == "pine" {
                    "Fronds"
                } else {
                    "Leaves"
                }
            ),
            "Woods",
            leaves_color,
            Pattern::Leaves,
        );
    }
    for (key, name, color) in STONES {
        for (suffix, label, pattern) in [
            ("", "", Pattern::Stone),
            ("_cobble", " Cobble", Pattern::Cobble),
            ("_bricks", " Bricks", Pattern::Bricks),
            ("_tiles", " Tiles", Pattern::Tiles),
            ("_carved", " Carved", Pattern::Carved),
        ] {
            uniform(
                &mut registry,
                format!("{key}{suffix}"),
                format!("{name}{label}"),
                "Stonework",
                color,
                pattern,
            );
        }
    }
    for (key, name, color) in COLORS {
        for (suffix, label, pattern) in [
            ("clay", "Clay", Pattern::Dirt),
            ("ceramic", "Ceramic", Pattern::Ceramic),
            ("fabric", "Fabric", Pattern::Fabric),
            ("glass", "Clouded Glass", Pattern::Glass),
            ("tiles", "Tiles", Pattern::Tiles),
        ] {
            uniform(
                &mut registry,
                format!("{key}_{suffix}"),
                format!("{name} {label}"),
                "Colors",
                color,
                pattern,
            );
        }
    }
    for (key, name, color) in METALS {
        for (suffix, label, pattern) in [
            ("plate", "Plate", Pattern::Metal),
            ("riveted", "Riveted", Pattern::Tiles),
            ("grille", "Grille", Pattern::Carved),
        ] {
            uniform(
                &mut registry,
                format!("{key}_{suffix}"),
                format!("{name} {label}"),
                "Metals",
                color,
                pattern,
            );
        }
    }
    for (key, name, color) in GEMS {
        uniform(
            &mut registry,
            format!("{key}_ore"),
            format!("{name} Ore"),
            "Minerals",
            color,
            Pattern::Ore,
        );
        uniform(
            &mut registry,
            format!("{key}_crystal"),
            format!("{name} Crystal"),
            "Minerals",
            color,
            Pattern::Crystal,
        );
    }
    for (key, name, color, pattern) in [
        ("snow", "Packed Snow", [226, 235, 230], Pattern::Sand),
        ("red_sand", "Red Sand", [192, 117, 72], Pattern::Sand),
        ("terracotta", "Terracotta", [179, 111, 78], Pattern::Dirt),
        ("peat", "Peat", [78, 75, 58], Pattern::Dirt),
        ("mud", "Mud", [103, 90, 69], Pattern::Dirt),
        ("gravel", "Gravel", [143, 140, 132], Pattern::Cobble),
        ("chalk", "Chalk", [220, 216, 191], Pattern::Stone),
        ("salt", "Rock Salt", [220, 209, 199], Pattern::Crystal),
        ("ice", "Frosted Ice", [151, 201, 214], Pattern::Glass),
        ("moss", "Moss Carpet", [101, 129, 65], Pattern::Leaves),
        ("lichen", "Lichen Rock", [162, 170, 110], Pattern::Stone),
        ("ash_soil", "Ash Soil", [103, 103, 111], Pattern::Dirt),
        ("dune_sand", "Dune Sand", [224, 195, 132], Pattern::Sand),
        ("rust_soil", "Rust Soil", [145, 79, 61], Pattern::Dirt),
        ("deep_slate", "Deep Slate", [53, 66, 81], Pattern::Marble),
        ("shellstone", "Shellstone", [207, 191, 160], Pattern::Cobble),
        ("clay_soil", "River Clay", [151, 149, 134], Pattern::Dirt),
    ] {
        uniform(
            &mut registry,
            key.to_owned(),
            name.to_owned(),
            "Terrain",
            color,
            pattern,
        );
    }
    registry.items = items::build_items(&mut registry);
    debug_assert_eq!(registry.catalog.len(), 300);
    debug_assert_eq!(registry.items.len(), 75);
    registry
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pack {
    format: u32,
    id: String,
    name: String,
    blocks: Vec<PackBlock>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackBlock {
    id: BlockId,
    key: String,
    name: String,
    color: [u8; 3],
    pattern: Pattern,
    #[serde(default = "yes")]
    solid: bool,
    #[serde(default)]
    category: Option<String>,
}

fn yes() -> bool {
    true
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == b'_'
                || character == b'-'
        })
}

fn label(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|character| character.is_ascii_graphic() || character == b' ')
}

fn apply_pack(registry: &mut Registry, source: &str, text: &str) -> Result<(), String> {
    let pack: Pack = serde_json::from_str(text).map_err(|error| format!("{source}: {error}"))?;
    if pack.format != 1 || !identifier(&pack.id) || !label(&pack.name, 48) {
        return Err(format!(
            "{source}: expected format 1 and a valid pack id/name"
        ));
    }
    for block in pack.blocks {
        let parts: Vec<_> = block.key.split(':').collect();
        if !(4096..=8191).contains(&block.id)
            || parts.len() != 2
            || parts[0] != pack.id
            || !identifier(parts[1])
            || !label(&block.name, 64)
        {
            return Err(format!("{source}: invalid block declaration {}", block.key));
        }
        if registry
            .blocks
            .get(usize::from(block.id))
            .is_some_and(Option::is_some)
            || registry.keys.contains_key(&block.key)
        {
            return Err(format!(
                "{source}: duplicate block ID or key for {}",
                block.key
            ));
        }
        if registry.textures.len() >= MAX_TEXTURE_LAYERS {
            return Err(format!(
                "{source}: material limit of {MAX_TEXTURE_LAYERS} texture layers exceeded"
            ));
        }
        let category = block.category.unwrap_or_else(|| pack.name.clone());
        if !label(&category, 48) {
            return Err(format!(
                "{source}: category must have 1 to 48 ASCII characters"
            ));
        }
        let layer = texture(registry, block.color, block.pattern, &block.key);
        insert(
            registry,
            BlockDef {
                id: block.id,
                key: block.key,
                name: block.name,
                category,
                solid: block.solid,
                opaque: true,
                top: layer,
                bottom: layer,
                side: layer,
            },
        );
        registry.report.mod_blocks += 1;
    }
    registry.report.packs += 1;
    Ok(())
}

fn pack_paths(path: &Path) -> Result<Vec<PathBuf>, String> {
    if !path.try_exists().map_err(|error| error.to_string())? {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "json")
        {
            paths.push(path);
        } else if path.is_dir() && path.join("pack.json").is_file() {
            paths.push(path.join("pack.json"));
        }
    }
    paths.sort();
    Ok(paths)
}

pub(crate) fn build_registry(path: &Path) -> Result<Registry, String> {
    let mut registry = builtin_registry();
    for path in pack_paths(path)? {
        let text =
            fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        apply_pack(&mut registry, &path.display().to_string(), &text)?;
    }
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_has_300_unique_blocks_and_75_actionable_items() {
        let registry = builtin_registry();
        assert_eq!(registry.catalog.len(), 300);
        assert_eq!(registry.keys.len(), 301);
        let mut names = std::collections::HashSet::new();
        for &id in &registry.catalog {
            let block = registry.blocks[usize::from(id)].as_ref().unwrap();
            assert!(names.insert(block.name.clone()));
            assert!((block.top as usize) < registry.textures.len());
        }
        assert_eq!(registry.items.len(), 75);
        let mut item_keys = std::collections::HashSet::new();
        for item in &registry.items {
            assert!(item_keys.insert(item.key.as_str()));
            assert!((item.layer as usize) < registry.textures.len());
            match item.action {
                crate::items::ItemAction::Place(id) => {
                    assert!(registry.blocks[usize::from(id)].is_some())
                }
                crate::items::ItemAction::Mine { radius } => assert!((0..=2).contains(&radius)),
                crate::items::ItemAction::Speed { seconds }
                | crate::items::ItemAction::Jump { seconds } => assert!(seconds > 0.0),
                crate::items::ItemAction::Plant { species } => {
                    assert!(species < TREE_SPECIES.len())
                }
            }
        }
        assert!(registry.textures.len() <= 512);
        assert_eq!(
            registry.keys["slopcraft:limestone"],
            crate::blocks::LIMESTONE
        );
        assert_eq!(registry.keys["slopcraft:snow"], crate::blocks::SNOW);
        assert_eq!(registry.keys["slopcraft:red_sand"], crate::blocks::RED_SAND);
        assert_eq!(
            registry.keys["slopcraft:terracotta"],
            crate::blocks::TERRACOTTA
        );
    }

    #[test]
    fn mod_validation_rejects_duplicates_and_unknown_patterns() {
        let mut registry = builtin_registry();
        let pack = r#"{"format":1,"id":"demo","name":"Demo","blocks":[{"id":4096,"key":"demo:tile","name":"Demo Tile","color":[90,130,160],"pattern":"tiles","solid":false}]}"#;
        apply_pack(&mut registry, "test", pack).unwrap();
        assert_eq!(registry.keys["demo:tile"], 4096);
        assert!(!registry.blocks[4096].as_ref().unwrap().solid);
        assert!(
            apply_pack(&mut registry, "duplicate", pack)
                .unwrap_err()
                .contains("duplicate")
        );
        assert!(
            apply_pack(
                &mut builtin_registry(),
                "bad",
                &pack.replace("tiles", "unknown")
            )
            .is_err()
        );
        assert!(apply_pack(&mut builtin_registry(), "bad", &pack.replace("4096", "7")).is_err());
        assert!(
            apply_pack(
                &mut builtin_registry(),
                "bad",
                &pack.replace("demo:tile", "other:tile")
            )
            .is_err()
        );
    }

    #[test]
    fn example_pack_loads_three_materials_without_altering_builtins() {
        let mut registry = builtin_registry();
        let initial_layers = registry.textures.len();
        apply_pack(
            &mut registry,
            "example",
            include_str!("../examples/mods/aurora-studio/pack.json"),
        )
        .unwrap();
        assert_eq!(registry.report.mod_blocks, 3);
        assert_eq!(registry.catalog.len(), 303);
        assert_eq!(registry.textures.len(), initial_layers + 3);
        assert_eq!(registry.keys["slopcraft:pine_log"], 5);
        assert_eq!(registry.keys["aurora:seafoam_bricks"], 4096);
    }
}
