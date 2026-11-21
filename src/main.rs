#![cfg_attr(windows, windows_subsystem = "windows")]

mod blocks;
mod content;
mod game;
mod inventory;
mod items;
mod mesh;
mod player;
mod render;
mod world;

use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use winit::event_loop::EventLoop;

use crate::{game::Game, world::World};

const DEFAULT_SEED: u64 = 184203;
const USAGE: &str = "Slopcraft 0.2\n\nUsage: slopcraft [--world PATH] [--seed NUMBER] [--data-dir PATH] [--shader PATH] [--verify]\n\n--world PATH     Open or create a world at this path.\n--seed NUMBER    Set the unsigned seed of a new world (default: 184203).\n--data-dir PATH  Use a separate saves, mods and shader packs directory.\n--shader PATH    Load a WGSL shader pack directory or file.\n--verify         Validate content and generate terrain without opening a window.\n--help           Show this help.\n";

struct Options {
    path: PathBuf,
    seed: u64,
    data_dir: PathBuf,
    shader: Option<PathBuf>,
    verify: bool,
}

impl Options {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Self>, String> {
        let mut args = args.into_iter();
        let mut path = None;
        let mut seed = None;
        let mut data_dir = None;
        let mut shader = None;
        let mut verify = false;
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--help" | "-h") => return Ok(None),
                Some("--world") if path.is_none() => {
                    let value = args.next().filter(|arg| !arg.is_empty());
                    path = Some(value.ok_or("--world needs a file path")?.into());
                }
                Some("--seed") if seed.is_none() => {
                    seed = Some(
                        args.next()
                            .and_then(|raw| raw.to_str()?.parse().ok())
                            .ok_or("--seed needs an unsigned 64-bit integer")?,
                    );
                }
                Some("--data-dir") if data_dir.is_none() => {
                    data_dir = Some(PathBuf::from(
                        args.next()
                            .filter(|arg| !arg.is_empty())
                            .ok_or("--data-dir needs a directory")?,
                    ));
                }
                Some("--shader") if shader.is_none() => {
                    shader = Some(PathBuf::from(
                        args.next()
                            .filter(|arg| !arg.is_empty())
                            .ok_or("--shader needs a pack directory or WGSL file")?,
                    ));
                }
                Some("--verify") if !verify => verify = true,
                _ => {
                    return Err(format!(
                        "Unrecognized or repeated argument {}. Run with --help.",
                        arg.to_string_lossy()
                    ));
                }
            }
        }

        let data_dir = data_dir.unwrap_or_else(slopcraft::data_directory);
        let path = path.unwrap_or_else(|| data_dir.join("world.json"));
        Ok(Some(Self {
            path,
            seed: seed.unwrap_or(DEFAULT_SEED),
            data_dir,
            shader,
            verify,
        }))
    }
}

fn run() -> Result<(), String> {
    let Some(options) = Options::parse(env::args_os().skip(1))? else {
        print!("{USAGE}");
        return Ok(());
    };
    prepare_data(&options.data_dir)?;
    let report = blocks::initialize(&options.data_dir.join("mods"))?;
    let content_label = format!(
        "{} blocks / {} items / {} mods",
        report.builtin_blocks + report.mod_blocks,
        report.items,
        report.packs
    );
    println!("{content_label}");
    let exists = options
        .path
        .try_exists()
        .map_err(|error| format!("{}: {error}", options.path.display()))?;
    let mut world = if exists {
        World::load(&options.path).map_err(|error| format!("Opening world failed: {error}"))?
    } else {
        World::new(options.seed)
    };
    println!(
        "Slopcraft | seed {} | {}",
        world.seed(),
        options.path.display()
    );
    if options.verify {
        let start = std::time::Instant::now();
        for z in -2..=2 {
            for x in -2..=2 {
                world.load_chunk(world::ChunkPos { x, z });
            }
        }
        let mut triangles = 0;
        for z in -1..=1 {
            for x in -1..=1 {
                triangles += mesh::build_chunk(&world, world::ChunkPos { x, z })
                    .indices
                    .len()
                    / 3;
            }
        }
        println!(
            "Verified generator {}, spawn {}, {} triangles, {:.1} ms",
            world.generator_version(),
            world.spawn(),
            triangles,
            start.elapsed().as_secs_f64() * 1000.0
        );
        return Ok(());
    }
    let event_loop = EventLoop::new().map_err(|error| error.to_string())?;
    let mut game = Game::new(
        world,
        options.path,
        exists,
        options.data_dir,
        content_label,
        options.shader,
    );
    event_loop
        .run_app(&mut game)
        .map_err(|error| error.to_string())?;
    game.result()
}

fn prepare_data(directory: &Path) -> Result<(), String> {
    fs::create_dir_all(directory.join("mods")).map_err(|error| error.to_string())?;
    let amber = directory.join("shaderpacks/amber");
    fs::create_dir_all(&amber).map_err(|error| error.to_string())?;
    let shader = amber.join("terrain.wgsl");
    if !shader.try_exists().map_err(|error| error.to_string())? {
        fs::write(shader, include_str!("../shaderpacks/amber/terrain.wgsl"))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn main() -> ExitCode {
    #[cfg(windows)]
    let console = windows_launch::attach_console();

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Slopcraft: {error}");
            #[cfg(windows)]
            if !console {
                windows_launch::show_error(&error);
            }
            ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
mod windows_launch {
    use std::ptr::null_mut;
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        System::Console::{ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE},
        UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
    };

    pub fn attach_console() -> bool {
        // Keep redirected handles, and attach only when started from an existing console.
        unsafe {
            let stderr = GetStdHandle(STD_ERROR_HANDLE);
            (!stderr.is_null() && stderr != INVALID_HANDLE_VALUE)
                || AttachConsole(ATTACH_PARENT_PROCESS) != 0
        }
    }

    pub fn show_error(error: &str) {
        let message: Vec<u16> = error.encode_utf16().chain([0]).collect();
        let title: Vec<u16> = "Slopcraft".encode_utf16().chain([0]).collect();
        // Both pointers remain valid until the synchronous dialog returns.
        unsafe {
            MessageBoxW(
                null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            )
        };
    }
}
