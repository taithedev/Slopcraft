//! Versioned Windows installs from this project's GitHub releases.

use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::blocking::{Client, Response};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const API: &str = "https://api.github.com/repos/taithedev/Slopcraft/releases/latest";
const DOWNLOAD_ROOT: &str = "https://github.com/taithedev/Slopcraft/releases/download/";
pub const ARCHIVE_NAME: &str = "Slopcraft-Windows-x64.zip";
const CHECKSUM_NAME: &str = "Slopcraft-Windows-x64.sha256";
const MAX_DOWNLOAD: u64 = 150 * 1024 * 1024;
const MAX_EXTRACTED: u64 = 300 * 1024 * 1024;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

pub struct AvailableUpdate {
    pub version: Version,
    archive: String,
    checksum: String,
}

#[derive(Serialize, Deserialize)]
struct Installed {
    version: Version,
}

pub fn client() -> Result<Client, String> {
    Client::builder()
        .user_agent(concat!("Slopcraft/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|error| error.to_string())
}

pub fn check(client: &Client, current: &Version) -> Result<Option<AvailableUpdate>, String> {
    let response = client
        .get(API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|error| format!("Update check failed: {error}"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let data = bounded(
        response
            .error_for_status()
            .map_err(|error| error.to_string())?,
        1024 * 1024,
    )?;
    let release: Release = serde_json::from_slice(&data).map_err(|error| error.to_string())?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|error| format!("Invalid release version: {error}"))?;
    if version <= *current || !version.pre.is_empty() {
        return Ok(None);
    }
    // Numeric version tags make the download location unambiguous.
    let expected_root = format!("{DOWNLOAD_ROOT}v{version}/");
    let asset = |name: &str, maximum: u64| -> Result<String, String> {
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == name)
            .ok_or_else(|| format!("Release is missing {name}"))?;
        if asset.size == 0
            || asset.size > maximum
            || asset.browser_download_url != format!("{expected_root}{name}")
        {
            return Err(format!(
                "Release asset {name} has an invalid location or size"
            ));
        }
        Ok(asset.browser_download_url.clone())
    };
    Ok(Some(AvailableUpdate {
        version,
        archive: asset(ARCHIVE_NAME, MAX_DOWNLOAD)?,
        checksum: asset(CHECKSUM_NAME, 4096)?,
    }))
}

fn bounded(response: Response, maximum: u64) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum)
    {
        return Err("Download exceeds the size limit".into());
    }
    let mut bytes = Vec::new();
    response
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > maximum {
        return Err("Download exceeds the size limit".into());
    }
    Ok(bytes)
}

pub fn install(
    client: &Client,
    update: &AvailableUpdate,
    data_dir: &Path,
) -> Result<PathBuf, String> {
    let checksum = bounded(
        client
            .get(&update.checksum)
            .send()
            .and_then(Response::error_for_status)
            .map_err(|error| error.to_string())?,
        4096,
    )?;
    let checksum = std::str::from_utf8(&checksum).map_err(|error| error.to_string())?;
    let checksum = parse_checksum(checksum)?;
    let archive = bounded(
        client
            .get(&update.archive)
            .timeout(Duration::from_secs(120))
            .send()
            .and_then(Response::error_for_status)
            .map_err(|error| error.to_string())?,
        MAX_DOWNLOAD,
    )?;
    verify_checksum(&archive, checksum)?;
    install_archive(&archive, &update.version, data_dir)
}

fn parse_checksum(text: &str) -> Result<&str, String> {
    let mut fields = text.split_whitespace();
    let checksum = fields.next().unwrap_or_default();
    let name = fields.next().unwrap_or_default().trim_start_matches('*');
    if checksum.len() != 64
        || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit())
        || name != ARCHIVE_NAME
        || fields.next().is_some()
    {
        return Err("Release checksum file is invalid".into());
    }
    Ok(checksum)
}

fn verify_checksum(bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(expected) {
        return Err("Downloaded update failed its SHA-256 check".into());
    }
    Ok(())
}

fn install_archive(bytes: &[u8], version: &Version, data_dir: &Path) -> Result<PathBuf, String> {
    let versions = data_dir.join("versions");
    fs::create_dir_all(&versions).map_err(|error| error.to_string())?;
    let destination = versions.join(version.to_string());
    let executable = destination.join("slopcraft.exe");
    if !destination.exists() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let stage = versions.join(format!(".staging-{}-{unique}", std::process::id()));
        fs::create_dir(&stage).map_err(|error| error.to_string())?;
        let result = extract_archive(bytes, &stage)
            .and_then(|()| fs::rename(&stage, &destination).map_err(|error| error.to_string()));
        if let Err(error) = result {
            // stage is a newly created directory directly below this install's versions directory.
            let _ = fs::remove_dir_all(&stage);
            return Err(error);
        }
    }
    if !executable.is_file() {
        return Err("Installed version is missing slopcraft.exe".into());
    }
    let temporary = data_dir.join("installed.json.tmp");
    let mut pointer = File::create(&temporary).map_err(|error| error.to_string())?;
    serde_json::to_writer(
        &mut pointer,
        &Installed {
            version: version.clone(),
        },
    )
    .map_err(|error| error.to_string())?;
    pointer.sync_all().map_err(|error| error.to_string())?;
    drop(pointer);
    fs::rename(temporary, data_dir.join("installed.json")).map_err(|error| error.to_string())?;
    Ok(executable)
}

fn extract_archive(bytes: &[u8], stage: &Path) -> Result<(), String> {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|error| error.to_string())?;
    if archive.len() > 256 {
        return Err("Update archive has too many files".into());
    }
    let mut total = 0;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|error| error.to_string())?;
        let relative = file
            .enclosed_name()
            .ok_or("Update archive contains an unsafe path")?;
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("Update archive contains a symbolic link".into());
        }
        // Reject NTFS alternate streams and Windows path aliases on every platform.
        if relative.components().any(|part| {
            let text = part.as_os_str().to_string_lossy();
            text.contains(':') || text.ends_with(['.', ' ']) || text.contains('\\')
        }) {
            return Err("Update archive contains an invalid Windows path".into());
        }
        total += file.size();
        if total > MAX_EXTRACTED {
            return Err("Expanded update exceeds the size limit".into());
        }
        let path = stage.join(relative);
        if file.is_dir() {
            fs::create_dir_all(path).map_err(|error| error.to_string())?;
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|error| error.to_string())?;
            let count = std::io::copy(&mut file.by_ref().take(MAX_EXTRACTED + 1), &mut output)
                .map_err(|error| error.to_string())?;
            if count > MAX_EXTRACTED {
                return Err("Expanded file exceeds the size limit".into());
            }
        }
    }
    if !stage.join("slopcraft.exe").is_file() {
        return Err("Update archive is missing slopcraft.exe".into());
    }
    Ok(())
}

pub fn installed(data_dir: &Path) -> Option<(Version, PathBuf)> {
    let pointer = File::open(data_dir.join("installed.json")).ok()?;
    let installed: Installed = serde_json::from_reader(pointer).ok()?;
    let executable = data_dir
        .join("versions")
        .join(installed.version.to_string())
        .join("slopcraft.exe");
    executable
        .is_file()
        .then_some((installed.version, executable))
}

pub fn lock(data_dir: &Path) -> Result<File, String> {
    fs::create_dir_all(data_dir).map_err(|error| error.to_string())?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    options
        .open(data_dir.join("launcher.lock"))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn archive(name: &str) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"fixture").unwrap();
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn checksum_requires_expected_file_and_rejects_corruption() {
        let hash = format!("{:x}", Sha256::digest(b"hello"));
        let file = format!("{hash}  {ARCHIVE_NAME}\n");
        assert_eq!(parse_checksum(&file).unwrap(), hash);
        assert!(verify_checksum(b"hello", &hash).is_ok());
        assert!(verify_checksum(b"changed", &hash).is_err());
        assert!(parse_checksum(&format!("{hash} other.zip")).is_err());
    }

    #[test]
    fn archive_rejects_path_traversal_and_missing_game() {
        let root = std::env::temp_dir().join(format!("slopcraft-archive-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        assert!(extract_archive(&archive("../escape.exe"), &root).is_err());
        assert!(extract_archive(&archive("readme.txt"), &root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn install_keeps_data_and_can_recover_the_version_pointer() {
        let root = std::env::temp_dir().join(format!("slopcraft-install-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("world.json"), "original save").unwrap();
        let version = Version::parse("0.2.0").unwrap();
        let path = install_archive(&archive("slopcraft.exe"), &version, &root).unwrap();
        assert!(path.is_file());
        assert_eq!(installed(&root).unwrap().0, version);
        assert_eq!(
            fs::read_to_string(root.join("world.json")).unwrap(),
            "original save"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
