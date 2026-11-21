# Slopcraft

An original voxel building and exploration game written in Rust with wgpu and winit. Slopcraft 0.2 expands the first Wildreach build with 300 blocks, 75 usable items, a searchable creative inventory, six biomes, caves, twelve tree species, content mods, reloadable shader packs, and a Windows update launcher.

The game uses original textures and item icons generated from code and runs as a native executable.

## Download and play

Download `Slopcraft-Windows-x64.zip` from [GitHub Releases](https://github.com/taithedev/Slopcraft/releases/latest), extract the complete ZIP, and open `SlopcraftLauncher.exe`. The launcher checks for the latest stable release, verifies its SHA-256 checksum, installs newer versions, and starts the game. You can also open `slopcraft.exe` directly.

Click **Play** to capture the mouse. The game targets 64-bit Windows with a DirectX 12 or Vulkan graphics driver and at least 512 texture-array layers. An RTX 3060 supports this configuration.

| Control | Action |
| --- | --- |
| WASD / mouse | Move / look |
| Space / Shift | Jump / sprint |
| Left mouse, hold to repeat | Break blocks or use the equipped mining tool |
| Right mouse, hold to repeat | Place blocks or use an item |
| 1–7 / wheel | Select a hotbar slot |
| E | Open creative inventory |
| Tab inside inventory | Switch between blocks and items |
| Type / Backspace | Search names, categories, and stable content keys |
| Arrows / Page Up / Page Down / wheel | Navigate inventory |
| Click an inventory card | Equip it in the current hotbar slot |
| Enter inside inventory | Equip the highlighted entry and return to play |
| Esc | Close inventory or pause / resume |
| F3 / F5 | Statistics / save |
| F6 / F7 | Reload current shader / cycle shader packs |

## Building and exploration

The 300 blocks include twelve wood species, sixteen stone families with five finishes each, sixteen color palettes, metals, ores, crystals, snow, red sand, and other landscape materials. All 75 items have original transparent icons. Everything in creative mode is unlimited.

The fifteen tools have three mining radii. Twelve foods and drinks provide timed movement or jump effects. Twelve saplings grow trees when used on soil with open space. Twelve pigments place colored clay, and twenty-four building supplies place planks or stonework.

New worlds contain meadows, woodlands, pine forest, alpine slopes, dunes, and badlands. Seeded terrain combines ridges, valleys, rock strata, caves, and trees with several canopy shapes. Chunks are 16×16×128 blocks. Face removal, greedy meshing, ambient corner shading, GPU mesh caching, frustum culling, and incremental chunk streaming keep rendering efficient.

## Worlds

The default data folder is `%LOCALAPPDATA%\Slopcraft`. It contains `world.json`, `mods`, `shaderpacks`, launcher logs, and installed update versions. Saves include the world seed, generator version, edited block identities, and player position and view direction. The game autosaves about every twelve seconds and saves when the window closes.

World files from Wildreach 0.1 retain their original terrain generator. Open one with `--world` to continue it. New worlds use generator 2 and the expanded terrain.

```powershell
.\slopcraft.exe --world .\saves\alpine.json --seed 8341
.\slopcraft.exe --data-dir .\MySlopcraftData
.\slopcraft.exe --world "$env:LOCALAPPDATA\Wildreach\world.json"
```

The default seed is `184203`. An existing world uses its saved seed. Use a distinct world path to create another world. Close a world before opening its file in another game instance.

## Mods and shader packs

Content packs load from the active data folder's `mods` directory at startup. JSON packs add named blocks with stable identifiers, collision properties, colors, and any of 21 generated material patterns. The loader validates the complete catalog before opening a world. Saves record namespaced block keys, so a missing required pack produces a clear load error.

Copy the included `examples/mods/aurora-studio` folder into `%LOCALAPPDATA%\Slopcraft\mods` and restart to add its three example blocks. The [content guide](docs/CONTENT.md) documents the format, limits, and stable identifiers.

Shader packs contain a `terrain.wgsl` file and share Slopcraft's documented GPU bindings. The Amber example is installed in the active shader folder on first launch. Press F7 to switch between Daylight and installed packs, or use `--shader PATH`. Press F6 after editing a pack to reload it. All affected GPU pipelines are validated before the working shader is replaced. The [shader guide](docs/SHADER_PACKS.md) includes the vertex layout and a complete example.

## Build from source on Windows

Install [Rust through rustup](https://rustup.rs/) and Microsoft's [C++ build tools with the Windows SDK](https://learn.microsoft.com/en-us/windows/dev-environment/rust/setup). Clone this repository and use PowerShell in the folder containing `Cargo.toml`.

```powershell
git clone https://github.com/taithedev/Slopcraft.git
cd Slopcraft
rustup toolchain install 1.99.0 --profile minimal --component clippy --component rustfmt
cargo build --release --locked --bins
cargo run --release --locked --bin slopcraft
```

The executables are `target\release\slopcraft.exe` and `target\release\launcher.exe`. Packaging renames the second file to `SlopcraftLauncher.exe`.

```powershell
cargo test --release --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
.\target\release\slopcraft.exe --verify --data-dir .\VerificationData | Out-Host
.\scripts\package.ps1
```

`--verify` validates installed content and generates and meshes a terrain sample without opening a game window. The packaging script creates a Windows ZIP and checksum in `dist`.

## Automatic releases and updates

The Windows GitHub Actions workflow checks formatting, checks code, tests, builds, verifies content, and packages each push to `main`. When `Cargo.toml` contains a version that has no release yet, it publishes a corresponding `vVERSION` release with the Windows ZIP and checksum. Existing releases retain their files. Increment the package version when publishing an update.

The launcher downloads release assets from `taithedev/Slopcraft`. Versioned installs live under the data folder's `versions` directory. It updates before starting a game, keeps world and mod folders outside the install, and starts the available game if an update check or download fails. `launcher.log` records update results. SHA-256 detects corrupted downloads. Releases are distributed through the repository's GitHub HTTPS endpoints.

## Source layout

| Module | Responsibility |
| --- | --- |
| `blocks.rs`, `content.rs`, `items.rs` | Stable registries, generated material recipes, JSON packs, item actions |
| `world.rs`, `terrain_legacy.rs`, `save.rs` | Terrain, trees, chunk data, generator compatibility, persistence |
| `mesh.rs` | Greedy meshing, corner shading, original textures and icons |
| `render.rs`, `render/`, `terrain.wgsl` | wgpu rendering, shader packs, inventory and HUD |
| `game.rs`, `inventory.rs`, `player.rs` | Input, item use, inventory, physics, targeting, streaming |
| `update.rs`, `bin/launcher.rs` | Release checks, checksum validation, extraction and launch |

Slopcraft 0.2 is a creative sandbox with expanded building and exploration features. Future gameplay can build on its block, item, save, and rendering interfaces. Dependency licenses and font credits are included in `THIRD_PARTY_NOTICES.txt`.
