# Content and block packs

Slopcraft includes 300 non-air blocks and 75 usable items. Every built-in block
has a permanent namespaced key and numeric ID. Existing Wildreach block IDs
0 through 7 retain their meanings.

## Built-in catalog

| Block group | Count | Contents |
| --- | --- | --- |
| Natural | 7 | Grass, loam, stone, sand, pine log, pine needles, slate |
| Woods | 60 | Twelve species with logs, planks, parquet, mosaics, and foliage |
| Stonework | 80 | Sixteen stones with raw, cobble, brick, tile, and carved finishes |
| Colors | 80 | Sixteen palettes with clay, ceramic, fabric, clouded glass, and tiles |
| Metals | 24 | Eight metals with plates, rivets, and grilles |
| Minerals | 32 | Sixteen ores and matching crystal blocks |
| Terrain | 17 | Snow, red sand, terracotta, peat, mud, gravel, chalk, salt, ice, moss, lichen, ash, dune sand, rust soil, deep slate, shellstone, river clay |

Clouded glass and frosted ice use opaque illustrated materials. All built-in
materials are original textures generated at startup.

| Item group | Count | Use |
| --- | --- | --- |
| Tools | 15 | Five tool shapes, each in copper, steel, and crystal. Their mining radii are 0, 1, and 2 blocks |
| Food | 12 | Six foods or drinks provide a timed speed effect and six provide a timed jump effect |
| Saplings | 12 | Plant pine, oak, birch, ash, maple, cedar, willow, acacia, cherry, walnut, aspen, or redwood |
| Pigments | 12 | Place the corresponding colored clay |
| Building | 24 | Twelve wood bundles place planks and twelve mason kits place stone bricks |

Creative inventory entries are unlimited. The item definition records the
action used by the game. Every item has an original transparent pixel icon.

## Install the example block pack

Copy the folder `examples/mods/aurora-studio` into the game's active mods folder.
Restart Slopcraft to load it. The example adds Seafoam Bricks, Rose Ceramic,
and Walk-through Mosaic. Its folder under `examples` keeps the example inactive
until you install it.

The loader reads `mods/*.json` and `mods/*/pack.json` in sorted path order.
It reads and validates the complete registry before opening a world.

## Define a pack

```json
{
  "format": 1,
  "id": "myworkshop",
  "name": "My Workshop",
  "blocks": [
    {
      "id": 4100,
      "key": "myworkshop:sea_tiles",
      "name": "Sea Tiles",
      "category": "My Workshop",
      "color": [72, 144, 168],
      "pattern": "tiles",
      "solid": true
    }
  ]
}
```

The pack's `id` and the part of each key after the colon allow lowercase ASCII
letters, digits, underscores, and hyphens. Both parts have a maximum of 64
characters. The key prefix must equal the pack ID. A block ID must be between
4096 and 8191 inclusive and must be unique across installed packs. A block key
must also be unique. Keep both values stable when updating a pack.

Block colors have three integer channels from 0 to 255. Block names use up to
64 ASCII characters. Categories use up to 48 ASCII characters and default to
the pack name. `solid` defaults to true. A false value makes an opaque block
walk-through. Its visible material remains in the world.

Supported patterns are `solid`, `stone`, `cobble`, `bricks`, `tiles`,
`carved`, `planks`, `parquet`, `mosaic`, `bark`, `rings`, `leaves`,
`sand`, `dirt`, `fabric`, `ceramic`, `metal`, `ore`, `marble`,
`crystal`, and `glass`. Materials use 16-by-16 pixels with repeated UVs.

The renderer supports 512 material layers. Built-ins and item icons use 389,
leaving 123 layers for pack blocks. Every pack block uses one layer. Loading
fails with a message when a declaration is malformed, a key or ID conflicts,
or this material budget is exceeded.

This format supports data-defined block content. Packs load at startup.
Additional pack formats, scripted behaviors, and item declarations can be
added through the registry without changing stable block identities.
