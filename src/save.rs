use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use glam::IVec3;
use serde::{Deserialize, Serialize};

use super::{ChunkPos, World, editable, index_in_chunk, position_of, valid_pose};
use crate::blocks::{BlockId, block_id, block_key};

const SAVE_VERSION: u32 = 2;
static FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize, Serialize)]
struct Save {
    version: u32,
    seed: u64,
    #[serde(default)]
    generator_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    player: Option<PlayerPose>,
    edits: Vec<Edit>,
}

#[derive(Deserialize, Serialize)]
struct PlayerPose {
    feet: [f32; 3],
    yaw: f32,
    pitch: f32,
}

#[derive(Deserialize, Serialize)]
struct Edit {
    position: [i32; 3],
    block: SavedBlock,
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
enum SavedBlock {
    Key(String),
    Legacy(BlockId),
}

pub(super) fn load(path: &Path) -> Result<World, String> {
    let context = |error: &dyn std::fmt::Display| format!("{}: {error}", path.display());
    let file = File::open(path).map_err(|error| context(&error))?;
    let save: Save =
        serde_json::from_reader(BufReader::new(file)).map_err(|error| context(&error))?;
    let generator = match (save.version, save.generator_version) {
        (1, None | Some(1)) => 1,
        (2, Some(version @ 1..=2)) => version,
        (1..=2, _) => return Err(context(&"missing or unsupported terrain generator version")),
        _ => {
            return Err(context(&format!(
                "unsupported world version {}",
                save.version
            )));
        }
    };
    let mut world = World::new(save.seed);
    world.generator_version = generator;
    if let Some(pose) = save.player {
        if !valid_pose(pose.feet, pose.yaw, pose.pitch) {
            return Err(context(&"invalid saved player position"));
        }
        world.player_pose = Some((pose.feet, pose.yaw, pose.pitch));
    }
    for edit in save.edits {
        let position = IVec3::from_array(edit.position);
        let block = match (save.version, edit.block) {
            (1, SavedBlock::Legacy(block @ 0..=7)) => block,
            (2, SavedBlock::Key(key)) => block_id(&key).ok_or_else(|| {
                context(&format!("missing block '{key}' at {position}. Install the mod that provides this block before opening this world"))
            })?,
            _ => return Err(context(&format!("invalid block identity at {position}"))),
        };
        if !editable(position) || (save.version == 1 && position.y >= 80) {
            return Err(context(&format!("invalid block edit at {position}")));
        }
        let chunk = ChunkPos::from_block(position.x, position.z);
        let edits = world.edits.entry(chunk).or_default();
        if edits.insert(index_in_chunk(position), block).is_some() {
            return Err(context(&format!("duplicate block edit at {position}")));
        }
    }
    Ok(world)
}

pub(super) fn write(world: &World, path: &Path) -> Result<(), String> {
    let context = |error: &dyn std::fmt::Display| format!("{}: {error}", path.display());
    let file_name = path
        .file_name()
        .ok_or_else(|| context(&"missing save file name"))?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| context(&error))?;
    }
    let mut edits = Vec::with_capacity(world.change_count());
    for (&chunk, chunk_edits) in &world.edits {
        for (&index, &block) in chunk_edits {
            let key = block_key(block)
                .ok_or_else(|| context(&format!("cannot save unknown block ID {block}")))?;
            edits.push(Edit {
                position: position_of(chunk, index).to_array(),
                block: SavedBlock::Key(key.to_owned()),
            });
        }
    }
    edits.sort_unstable_by_key(|edit| edit.position);
    let data = Save {
        version: SAVE_VERSION,
        seed: world.seed(),
        generator_version: Some(world.generator_version()),
        player: world
            .player_pose()
            .map(|(feet, yaw, pitch)| PlayerPose { feet, yaw, pitch }),
        edits,
    };
    let mut temporary_name = OsString::from(file_name);
    temporary_name.push(format!(".{}.tmp", unique_suffix()));
    let temporary = path.with_file_name(temporary_name);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| context(&error))?;
    // A failed write keeps the previous complete save available.
    let result = flush_save(&data, file)
        .and_then(|()| fs::rename(&temporary, path).map_err(|error| error.to_string()));
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(context(&error));
    }
    Ok(())
}

fn flush_save(data: &Save, file: File) -> Result<(), String> {
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, data).map_err(|error| error.to_string())?;
    writer.write_all(b"\n").map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())?;
    writer
        .get_ref()
        .sync_all()
        .map_err(|error| error.to_string())
}

fn unique_suffix() -> String {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let sequence = FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}-{sequence}", std::process::id(), time.as_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{AIR, LOG, SNOW};
    use std::path::PathBuf;
    fn temporary_path() -> PathBuf {
        std::env::temp_dir().join(format!("slopcraft-test-{}.json", unique_suffix()))
    }

    #[test]
    fn saving_twice_preserves_block_keys_player_and_generator() {
        let path = temporary_path();
        let mut world = World::new(108);
        let removed = IVec3::new(-123, 8, -17);
        let placed = IVec3::new(16, 120, 31);
        world.save(&path).unwrap();
        assert!(world.set_block(removed, AIR));
        assert!(world.set_block(placed, SNOW));
        assert!(world.set_player_pose([2.5, 52.0, -5.5], 0.4, -0.2));
        world.save(&path).unwrap();
        let loaded = World::load(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert!(text.contains("slopcraft:snow"));
        assert_eq!(loaded.seed(), world.seed());
        assert_eq!(loaded.generator_version(), 2);
        assert_eq!(loaded.player_pose(), world.player_pose());
        assert_eq!(loaded.change_count(), 2);
        assert_eq!(loaded.block(removed), AIR);
        assert_eq!(loaded.block(placed), SNOW);
    }

    #[test]
    fn legacy_saves_keep_the_original_generator_when_resaved() {
        let path = temporary_path();
        fs::write(
            &path,
            r#"{"version":1,"seed":184203,"edits":[{"position":[16,72,31],"block":5}]}"#,
        )
        .unwrap();
        let world = World::load(&path).unwrap();
        assert_eq!(world.generator_version(), 1);
        assert_eq!(world.spawn(), glam::Vec3::new(0.5, 34.01, 0.5));
        assert_eq!(world.block(IVec3::new(16, 72, 31)), LOG);
        world.save(&path).unwrap();
        let restored = World::load(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(restored.generator_version(), 1);
        assert_eq!(restored.spawn(), world.spawn());
        for z in -32..32 {
            for x in -32..32 {
                for y in [7, 17, 27, 37, 47, 57, 67, 77] {
                    assert_eq!(
                        world.block(IVec3::new(x, y, z)),
                        restored.block(IVec3::new(x, y, z))
                    );
                }
            }
        }
    }

    #[test]
    fn missing_mod_blocks_and_invalid_data_fail_clearly() {
        let path = temporary_path();
        fs::write(&path, r#"{"version":2,"generator_version":2,"seed":42,"edits":[{"position":[0,55,0],"block":"missing:violet_tile"}]}"#).unwrap();
        let missing = World::load(&path).err().unwrap();
        fs::write(
            &path,
            r#"{"version":1,"seed":42,"edits":[{"position":[0,0,0],"block":0}]}"#,
        )
        .unwrap();
        let unsafe_edit = World::load(&path).err().unwrap();
        fs::write(&path, r#"{"version":3,"seed":42,"edits":[]}"#).unwrap();
        let unsupported = World::load(&path).err().unwrap();
        fs::remove_file(path).unwrap();
        assert!(missing.contains("missing block 'missing:violet_tile'"));
        assert!(unsafe_edit.contains("invalid block edit"));
        assert!(unsupported.contains("unsupported world version"));
    }
}
