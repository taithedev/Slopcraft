pub mod update;

use std::{env, path::PathBuf};

pub fn data_directory() -> PathBuf {
    env::var_os("LOCALAPPDATA").map_or_else(
        || PathBuf::from("SlopcraftData"),
        |folder| PathBuf::from(folder).join("Slopcraft"),
    )
}
