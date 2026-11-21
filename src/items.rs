use crate::{
    blocks::{BlockId, ItemIcon, Pattern, Registry, TextureSpec, registry},
    content::TREE_SPECIES,
};

#[derive(Clone, Copy, Debug)]
pub enum ItemAction {
    Place(BlockId),
    Mine { radius: i32 },
    Speed { seconds: f32 },
    Jump { seconds: f32 },
    Plant { species: usize },
}

pub struct ItemDef {
    pub name: String,
    pub key: String,
    pub layer: u32,
    pub action: ItemAction,
    pub category: String,
}

pub fn item_defs() -> &'static [ItemDef] {
    &registry().items
}

fn add(
    registry: &mut Registry,
    output: &mut Vec<ItemDef>,
    name: String,
    category: &str,
    icon: ItemIcon,
    color: [u8; 3],
    action: ItemAction,
) {
    let layer = registry.textures.len() as u32;
    let key = format!("slopcraft:{}", name.to_ascii_lowercase().replace(' ', "_"));
    registry.textures.push(TextureSpec {
        color,
        pattern: Pattern::Solid,
        seed: layer * 7919,
        legacy: None,
        icon: Some(icon),
    });
    output.push(ItemDef {
        name,
        key,
        layer,
        action,
        category: category.to_owned(),
    });
}

pub(crate) fn build_items(registry: &mut Registry) -> Vec<ItemDef> {
    let mut output = Vec::with_capacity(75);
    for (tool, icon) in [
        ("Pickaxe", ItemIcon::Pickaxe),
        ("Hammer", ItemIcon::Hammer),
        ("Shovel", ItemIcon::Shovel),
        ("Axe", ItemIcon::Axe),
        ("Drill", ItemIcon::Drill),
    ] {
        for (tier, radius, color) in [
            ("Copper", 0, [194, 128, 85]),
            ("Steel", 1, [172, 196, 207]),
            ("Crystal", 2, [117, 200, 188]),
        ] {
            add(
                registry,
                &mut output,
                format!("{tier} {tool}"),
                "Tools",
                icon,
                color,
                ItemAction::Mine { radius },
            );
        }
    }
    for (index, name) in [
        "Trail Apple",
        "Honey Pear",
        "Citrus Slice",
        "Mint Tea",
        "Berry Tart",
        "Spiced Cocoa",
    ]
    .into_iter()
    .enumerate()
    {
        add(
            registry,
            &mut output,
            name.to_owned(),
            "Food",
            if index >= 3 {
                ItemIcon::Bottle
            } else {
                ItemIcon::Food
            },
            [
                [204, 80, 69],
                [214, 185, 96],
                [228, 153, 61],
                [106, 175, 130],
                [165, 99, 165],
                [150, 100, 70],
            ][index],
            ItemAction::Speed {
                seconds: 20.0 + index as f32 * 4.0,
            },
        );
    }
    for (index, name) in [
        "Cloud Plum",
        "Spring Melon",
        "Ginger Bun",
        "Dew Elixir",
        "Skyberry Juice",
        "Feather Tonic",
    ]
    .into_iter()
    .enumerate()
    {
        add(
            registry,
            &mut output,
            name.to_owned(),
            "Food",
            if index >= 3 {
                ItemIcon::Bottle
            } else {
                ItemIcon::Food
            },
            [
                [140, 116, 190],
                [156, 194, 102],
                [207, 162, 111],
                [118, 190, 207],
                [172, 143, 203],
                [218, 216, 167],
            ][index],
            ItemAction::Jump {
                seconds: 20.0 + index as f32 * 4.0,
            },
        );
    }
    for (species, (_, name, color)) in TREE_SPECIES.into_iter().enumerate() {
        add(
            registry,
            &mut output,
            format!("{name} Sapling"),
            "Saplings",
            ItemIcon::Sapling,
            [color[0] / 2 + 20, color[1] / 2 + 70, color[2] / 2 + 20],
            ItemAction::Plant { species },
        );
    }
    for (name, color) in [
        ("Ivory", [224, 219, 200]),
        ("Amber", [226, 162, 60]),
        ("Coral", [212, 116, 104]),
        ("Scarlet", [180, 67, 69]),
        ("Rose", [213, 142, 168]),
        ("Orchid", [163, 115, 189]),
        ("Violet", [117, 87, 165]),
        ("Indigo", [82, 98, 162]),
        ("Azure", [71, 141, 186]),
        ("Teal", [63, 155, 150]),
        ("Mint", [123, 187, 151]),
        ("Moss", [105, 139, 80]),
    ] {
        let block = registry.keys[&format!("slopcraft:{}_clay", name.to_ascii_lowercase())];
        add(
            registry,
            &mut output,
            format!("{name} Pigment"),
            "Pigments",
            ItemIcon::Dye,
            color,
            ItemAction::Place(block),
        );
    }
    for (species, name, color) in TREE_SPECIES {
        let block = registry.keys[&format!("slopcraft:{species}_planks")];
        add(
            registry,
            &mut output,
            format!("{name} Building Bundle"),
            "Building",
            ItemIcon::Material,
            color,
            ItemAction::Place(block),
        );
    }
    for (name, color) in [
        ("Granite", [160, 130, 123]),
        ("Basalt", [76, 85, 91]),
        ("Marble", [205, 209, 205]),
        ("Limestone", [193, 185, 147]),
        ("Sandstone", [202, 170, 116]),
        ("Andesite", [132, 138, 139]),
        ("Diorite", [177, 182, 176]),
        ("Gabbro", [91, 104, 106]),
        ("Quartzite", [196, 177, 167]),
        ("Travertine", [202, 186, 146]),
        ("Onyx", [60, 68, 77]),
        ("Obsidian", [65, 56, 87]),
    ] {
        let block = registry.keys[&format!("slopcraft:{}_bricks", name.to_ascii_lowercase())];
        add(
            registry,
            &mut output,
            format!("{name} Mason Kit"),
            "Building",
            ItemIcon::Material,
            color,
            ItemAction::Place(block),
        );
    }
    output
}
