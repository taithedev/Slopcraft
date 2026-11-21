#![cfg_attr(windows, windows_subsystem = "windows")]

use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    process::{Command, ExitCode},
};

use semver::Version;
use slopcraft::{data_directory, update};

fn launch() -> Result<(), String> {
    let data_dir = data_directory();
    let own_version =
        Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| error.to_string())?;
    let bundled = env::current_exe()
        .map_err(|error| error.to_string())?
        .parent()
        .ok_or("Launcher has no parent directory")?
        .join("slopcraft.exe");
    let (mut version, mut executable) = update::installed(&data_dir)
        .filter(|(version, _)| version >= &own_version)
        .unwrap_or((own_version, bundled));
    if let Ok(_lock) = update::lock(&data_dir) {
        match update::client().and_then(|client| {
            if let Some(available) = update::check(&client, &version)? {
                let installed = update::install(&client, &available, &data_dir)?;
                version = available.version;
                executable = installed;
            }
            Ok(())
        }) {
            Ok(()) => log(&data_dir, &format!("Starting {version}")),
            Err(error) => log(
                &data_dir,
                &format!("Update postponed: {error}. Starting {version}."),
            ),
        }
    }
    if !executable.is_file() {
        return Err("The game executable is missing. Extract the complete Slopcraft release beside the launcher.".into());
    }
    Command::new(&executable)
        .args(env::args_os().skip(1))
        .current_dir(executable.parent().ok_or("Game directory is missing")?)
        .spawn()
        .map_err(|error| format!("Could not start {}: {error}", executable.display()))?;
    Ok(())
}

fn log(directory: &std::path::Path, message: &str) {
    if fs::create_dir_all(directory).is_ok() {
        if let Ok(mut log) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("launcher.log"))
        {
            let _ = writeln!(log, "{message}");
        }
    }
}

fn main() -> ExitCode {
    match launch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            log(&data_directory(), &error);
            eprintln!("Slopcraft: {error}");
            #[cfg(windows)]
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    MB_ICONERROR, MB_OK, MessageBoxW,
                };
                let text: Vec<u16> = error.encode_utf16().chain([0]).collect();
                MessageBoxW(
                    std::ptr::null_mut(),
                    text.as_ptr(),
                    windows_sys::w!("Slopcraft Launcher"),
                    MB_OK | MB_ICONERROR,
                );
            }
            ExitCode::FAILURE
        }
    }
}
