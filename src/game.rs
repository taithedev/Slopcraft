use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use glam::IVec3;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

use crate::{
    blocks::{AIR, HOTBAR},
    inventory::{Entry, Inventory, PAGE_SIZE},
    items::{ItemAction, item_defs},
    mesh::{build_chunk, generate_textures},
    player::{Movement, Player},
    render::{Frame, InventoryView, Renderer, Slot, inventory_cell},
    world::{ChunkPos, World},
};

const VIEW_RADIUS: i32 = 8;
const STARTUP_CHUNKS: usize = 25;
const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(12);

pub struct Game {
    world: World,
    save_path: PathBuf,
    dirty: bool,
    last_save_attempt: Instant,
    player: Player,
    controls: Controls,
    selected: usize,
    slots: [Slot<'static>; HOTBAR.len()],
    hotbar: [Entry; HOTBAR.len()],
    inventory: Inventory,
    cursor: (f32, f32),
    shader_packs: Vec<PathBuf>,
    shader_index: usize,
    shader_name: String,
    active_shader: Option<PathBuf>,
    initial_shader: Option<PathBuf>,
    data_dir: PathBuf,
    content_label: String,
    last_edit: Instant,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    chunks: HashMap<ChunkPos, usize>,
    cached: HashSet<ChunkPos>,
    pending: VecDeque<ChunkPos>,
    center: Option<ChunkPos>,
    paused: bool,
    status: String,
    status_until: Instant,
    debug: bool,
    time: Instant,
    fps_since: Instant,
    frames: u32,
    fps: f32,
    error: Option<String>,
}

#[derive(Default)]
struct Controls {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    sprint: bool,
    jump: bool,
    scroll: f32,
    breaking: bool,
    placing: bool,
}

impl Game {
    pub fn new(
        world: World,
        save_path: PathBuf,
        exists: bool,
        data_dir: PathBuf,
        content_label: String,
        initial_shader: Option<PathBuf>,
    ) -> Self {
        let player = world
            .player_pose()
            .and_then(|(feet, yaw, pitch)| Player::restore(&world, feet, yaw, pitch))
            .unwrap_or_else(|| Player::new(world.spawn()));
        let now = Instant::now();
        Self {
            world,
            save_path,
            dirty: !exists,
            last_save_attempt: now,
            player,
            controls: Controls::default(),
            selected: 0,
            slots: HOTBAR.map(|id| Entry::Block(id).slot()),
            hotbar: HOTBAR.map(Entry::Block),
            inventory: Inventory::default(),
            cursor: (0.0, 0.0),
            shader_packs: Self::shader_paths(&data_dir.join("shaderpacks")),
            shader_index: 0,
            shader_name: "Daylight".into(),
            active_shader: None,
            initial_shader,
            data_dir,
            content_label,
            last_edit: now,
            window: None,
            renderer: None,
            chunks: HashMap::new(),
            cached: HashSet::new(),
            pending: VecDeque::new(),
            center: None,
            paused: true,
            status: String::new(),
            status_until: now,
            debug: false,
            time: now,
            fps_since: now,
            frames: 0,
            fps: 0.0,
            error: None,
        }
    }

    pub fn result(self) -> Result<(), String> {
        self.error.map_or(Ok(()), Err)
    }

    fn show_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.status_until = Instant::now() + Duration::from_secs(3);
    }

    fn save(&mut self, manual: bool) -> Result<(), String> {
        self.last_save_attempt = Instant::now();
        let (feet, yaw, pitch) = self.player.pose();
        if self.world.player_pose() != Some((feet, yaw, pitch)) {
            self.world.set_player_pose(feet, yaw, pitch);
            self.dirty = true;
        }
        if self.dirty {
            let result = self.world.save(&self.save_path);
            if let Err(error) = result {
                self.show_status("SAVE FAILED - CHECK WORLD FILE");
                let message = format!("Saving world failed: {error}");
                eprintln!("Slopcraft: {message}");
                return Err(message);
            }
            self.dirty = false;
        }
        if manual {
            self.show_status("WORLD SAVED");
        }
        Ok(())
    }

    fn set_paused(&mut self, paused: bool) {
        let Some(window) = &self.window else { return };
        if paused {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
        } else if window
            .set_cursor_grab(CursorGrabMode::Confined)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked))
            .is_err()
        {
            self.show_status("CLICK TO CAPTURE THE MOUSE");
            return;
        }
        window.set_cursor_visible(paused);
        self.paused = paused;
        self.controls = Controls::default();
        self.time = Instant::now();
        self.fps_since = self.time;
        self.frames = 0;
        self.fps = 0.0;
    }

    fn shader_paths(directory: &Path) -> Vec<PathBuf> {
        let mut paths: Vec<_> = std::fs::read_dir(directory)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.join("terrain.wgsl").is_file())
            .collect();
        paths.sort();
        paths
    }

    fn reload_shader(&mut self, cycle: bool) {
        let candidate = if cycle {
            self.shader_packs = Self::shader_paths(&self.data_dir.join("shaderpacks"));
            self.shader_index = (self.shader_index + 1) % (self.shader_packs.len() + 1);
            self.shader_index
                .checked_sub(1)
                .map(|index| self.shader_packs[index].clone())
        } else {
            self.active_shader.clone()
        };
        let result = if let Some(path) = &candidate {
            self.renderer.as_mut().unwrap().reload_shader(path)
        } else {
            self.renderer
                .as_mut()
                .unwrap()
                .use_builtin_shader()
                .map(|()| "Daylight".to_string())
        };
        match result {
            Ok(name) => {
                self.shader_name = name;
                self.active_shader = candidate;
                self.show_status(format!("SHADER - {}", self.shader_name));
            }
            Err(error) => {
                eprintln!("Shader reload: {error}");
                self.show_status("SHADER ERROR - KEPT CURRENT SHADER");
            }
        }
    }

    fn equip(&mut self, entry: Entry) {
        self.hotbar[self.selected] = entry;
        self.slots[self.selected] = entry.slot();
        self.show_status(format!("EQUIPPED {}", self.slots[self.selected].name));
    }

    fn close_inventory(&mut self) {
        self.inventory.open = false;
        self.set_paused(false);
    }

    fn update_center(&mut self) {
        let cell = self.player.feet().floor().as_ivec3();
        let center = ChunkPos::from_block(cell.x, cell.z);
        if self.center == Some(center) {
            return;
        }
        self.center = Some(center);
        let distance = |pos: ChunkPos| (pos.x - center.x).abs().max((pos.z - center.z).abs());
        let outside: Vec<_> = self
            .chunks
            .keys()
            .copied()
            .filter(|pos| distance(*pos) > VIEW_RADIUS)
            .collect();
        for pos in outside {
            self.chunks.remove(&pos);
            self.renderer.as_mut().unwrap().remove(pos);
        }
        self.cached.retain(|&pos| {
            if distance(pos) <= VIEW_RADIUS + 1 {
                true
            } else {
                self.world.unload_chunk(pos);
                false
            }
        });

        let mut pending = Vec::new();
        for z in -VIEW_RADIUS..=VIEW_RADIUS {
            for x in -VIEW_RADIUS..=VIEW_RADIUS {
                let pos = ChunkPos {
                    x: center.x + x,
                    z: center.z + z,
                };
                if !self.chunks.contains_key(&pos) {
                    pending.push(((x * x + z * z, z, x), pos));
                }
            }
        }
        pending.sort_unstable_by_key(|&(priority, _)| priority);
        self.pending = pending.into_iter().map(|(_, pos)| pos).collect();
    }

    fn stream(&mut self, max_chunks: usize, budget: Duration) {
        let start = Instant::now();
        for _ in 0..max_chunks {
            let Some(pos) = self.pending.pop_front() else {
                break;
            };
            // A one-chunk halo keeps edge samples and ambient occlusion on the cached path.
            for z in pos.z - 1..=pos.z + 1 {
                for x in pos.x - 1..=pos.x + 1 {
                    let neighbor = ChunkPos { x, z };
                    if self.cached.insert(neighbor) {
                        self.world.load_chunk(neighbor);
                    }
                }
            }
            self.rebuild(pos);
            if start.elapsed() >= budget {
                break;
            }
        }
    }

    fn rebuild(&mut self, pos: ChunkPos) {
        let mesh = build_chunk(&self.world, pos);
        self.chunks.insert(pos, mesh.indices.len() / 3);
        self.renderer.as_mut().unwrap().upload(pos, mesh);
    }

    fn edit(&mut self, button: MouseButton) {
        let entry = self.hotbar[self.selected];
        if button == MouseButton::Right {
            if let Entry::Item(index) = entry {
                match item_defs()[index].action {
                    ItemAction::Speed { seconds } => {
                        self.player.boost_speed(seconds);
                        self.show_status(format!("SPEED BOOST - {seconds:.0} SECONDS"));
                        return;
                    }
                    ItemAction::Jump { seconds } => {
                        self.player.boost_jump(seconds);
                        self.show_status(format!("JUMP BOOST - {seconds:.0} SECONDS"));
                        return;
                    }
                    _ => (),
                }
            }
        }
        let Some(hit) = self.player.target(&self.world) else {
            return;
        };
        if let Entry::Item(index) = entry {
            match (button, item_defs()[index].action) {
                (MouseButton::Left, ItemAction::Mine { radius }) => {
                    let mut changed = Vec::new();
                    for z in -radius..=radius {
                        for y in -radius..=radius {
                            for x in -radius..=radius {
                                if x * x + y * y + z * z > radius * radius {
                                    continue;
                                }
                                let cell = hit.block + IVec3::new(x, y, z);
                                if self.world.set_block(cell, AIR) {
                                    changed.push(cell);
                                }
                            }
                        }
                    }
                    self.changed(changed);
                    return;
                }
                (MouseButton::Right, ItemAction::Plant { species }) => {
                    let Some(base) = hit.adjacent else { return };
                    if (base.as_vec3() - self.player.feet()).with_y(0.0).length() < 3.5 {
                        self.show_status("PLANT A LITTLE FURTHER AWAY");
                        return;
                    }
                    let changed = self.world.plant_tree(base, species);
                    if changed.is_empty() {
                        self.show_status("SAPLINGS NEED OPEN SPACE ABOVE SOIL");
                    }
                    self.changed(changed);
                    return;
                }
                _ => (),
            }
        }
        let (cell, block) = match button {
            MouseButton::Left => (hit.block, AIR),
            MouseButton::Right => match hit.adjacent {
                Some(cell) if !self.player.overlaps(cell) => {
                    let block = match entry {
                        Entry::Block(id) => id,
                        Entry::Item(index) => match item_defs()[index].action {
                            ItemAction::Place(id) => id,
                            _ => return,
                        },
                    };
                    (cell, block)
                }
                _ => return,
            },
            _ => return,
        };
        if !self.world.set_block(cell, block) {
            return;
        }
        self.changed(vec![cell]);
    }

    fn changed(&mut self, cells: Vec<IVec3>) {
        if cells.is_empty() {
            return;
        }
        self.dirty = true;
        let mut affected = HashSet::new();
        for cell in cells {
            let first = ChunkPos::from_block(cell.x - 1, cell.z - 1);
            let last = ChunkPos::from_block(cell.x + 1, cell.z + 1);
            for z in first.z..=last.z {
                for x in first.x..=last.x {
                    affected.insert(ChunkPos { x, z });
                }
            }
        }
        for pos in affected {
            if self.chunks.contains_key(&pos) {
                self.rebuild(pos);
            }
        }
    }

    fn keyboard(&mut self, key: KeyCode, state: ElementState, repeat: bool) {
        let pressed = state.is_pressed();
        if self.inventory.open {
            if pressed {
                match key {
                    KeyCode::Escape => self.close_inventory(),
                    KeyCode::Tab if !repeat => self.inventory.switch_tab(),
                    KeyCode::Backspace => self.inventory.backspace(),
                    KeyCode::ArrowLeft => self.inventory.navigate(-1),
                    KeyCode::ArrowRight => self.inventory.navigate(1),
                    KeyCode::ArrowUp => self.inventory.navigate(-8),
                    KeyCode::ArrowDown => self.inventory.navigate(8),
                    KeyCode::PageUp => self.inventory.navigate(-(PAGE_SIZE as isize)),
                    KeyCode::PageDown => self.inventory.navigate(PAGE_SIZE as isize),
                    KeyCode::Enter => {
                        if let Some(entry) = self.inventory.selection() {
                            self.equip(entry);
                            self.close_inventory();
                        }
                    }
                    KeyCode::Digit1 => self.selected = 0,
                    KeyCode::Digit2 => self.selected = 1,
                    KeyCode::Digit3 => self.selected = 2,
                    KeyCode::Digit4 => self.selected = 3,
                    KeyCode::Digit5 => self.selected = 4,
                    KeyCode::Digit6 => self.selected = 5,
                    KeyCode::Digit7 => self.selected = 6,
                    _ => (),
                }
            }
            return;
        }
        if pressed && !repeat {
            match key {
                KeyCode::Escape => self.set_paused(!self.paused),
                KeyCode::KeyE => {
                    self.set_paused(true);
                    self.inventory.open();
                }
                KeyCode::F3 => self.debug = !self.debug,
                KeyCode::F5 => {
                    let _ = self.save(true);
                }
                KeyCode::F6 => self.reload_shader(false),
                KeyCode::F7 => self.reload_shader(true),
                KeyCode::Digit1 => self.selected = 0,
                KeyCode::Digit2 => self.selected = 1,
                KeyCode::Digit3 => self.selected = 2,
                KeyCode::Digit4 => self.selected = 3,
                KeyCode::Digit5 => self.selected = 4,
                KeyCode::Digit6 => self.selected = 5,
                KeyCode::Digit7 => self.selected = 6,
                _ => (),
            }
        }
        if !self.paused {
            match key {
                KeyCode::KeyW => self.controls.forward = pressed,
                KeyCode::KeyS => self.controls.backward = pressed,
                KeyCode::KeyA => self.controls.left = pressed,
                KeyCode::KeyD => self.controls.right = pressed,
                KeyCode::ShiftLeft | KeyCode::ShiftRight => self.controls.sprint = pressed,
                KeyCode::Space if pressed && !repeat => self.controls.jump = true,
                _ => (),
            }
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = now.duration_since(self.time).as_secs_f32();
        self.time = now;
        if !self.paused {
            let movement = Movement {
                forward: f32::from(u8::from(self.controls.forward))
                    - f32::from(u8::from(self.controls.backward)),
                right: f32::from(u8::from(self.controls.right))
                    - f32::from(u8::from(self.controls.left)),
                sprint: self.controls.sprint,
                jump: std::mem::take(&mut self.controls.jump),
            };
            self.player.tick(&self.world, &movement, dt);
            if self.last_edit.elapsed() >= Duration::from_millis(180) {
                let button = if self.controls.breaking {
                    Some(MouseButton::Left)
                } else if self.controls.placing {
                    Some(MouseButton::Right)
                } else {
                    None
                };
                if let Some(button) = button {
                    self.edit(button);
                    self.last_edit = now;
                }
            }
            if self.player.feet().y < -8.0 {
                self.player = Player::new(self.world.spawn());
            }
        }
        self.update_center();
        self.stream(3, Duration::from_millis(4));
        if self.last_save_attempt.elapsed() >= AUTOSAVE_INTERVAL {
            let _ = self.save(false);
        }

        let sample = self.fps_since.elapsed().as_secs_f32();
        if sample >= 0.5 {
            self.fps = self.frames as f32 / sample;
            self.frames = 0;
            self.fps_since = now;
        }
        let debug = if self.debug {
            let location = self.player.feet();
            format!(
                "{:.0} FPS | {} CHUNKS | {} TRIANGLES\nXYZ {:.1} {:.1} {:.1} | SEED {} | {} EDITS",
                self.fps,
                self.chunks.len(),
                self.chunks.values().sum::<usize>(),
                location.x,
                location.y,
                location.z,
                self.world.seed(),
                self.world.change_count()
            )
        } else {
            String::new()
        };
        let cards = self.inventory.cards();
        let environment = format!(
            "{} | {} | {}",
            self.world
                .biome_name(self.player.feet().x as i32, self.player.feet().z as i32),
            self.shader_name,
            self.content_label
        );
        let frame = Frame {
            eye: self.player.eye(),
            direction: self.player.direction(),
            selected: self.selected,
            slots: &self.slots,
            paused: self.paused,
            status: if now <= self.status_until {
                &self.status
            } else {
                ""
            },
            target: (!self.paused)
                .then(|| self.player.target(&self.world))
                .flatten()
                .map(|hit| hit.block),
            debug: &debug,
            view_distance: (VIEW_RADIUS * 16) as f32,
            environment: &environment,
            inventory: self.inventory.open.then_some(InventoryView {
                title: if self.inventory.items {
                    "ITEMS"
                } else {
                    "BLOCKS"
                },
                query: &self.inventory.query,
                entries: &cards,
                page: self.inventory.page(),
                pages: self.inventory.pages(),
                selected: self.inventory.local_selected(),
            }),
        };
        match self.renderer.as_mut().unwrap().draw(&frame) {
            Ok(()) => self.frames += 1,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                let size = self.window.as_ref().unwrap().inner_size();
                self.renderer
                    .as_mut()
                    .unwrap()
                    .resize(size.width, size.height);
                self.window.as_ref().unwrap().request_redraw();
            }
            Err(wgpu::SurfaceError::Timeout | wgpu::SurfaceError::Other) => (),
            Err(wgpu::SurfaceError::OutOfMemory) => {
                let _ = self.save(false);
                self.error = Some("The graphics device ran out of memory".into());
                event_loop.exit();
            }
        }
    }
}

impl ApplicationHandler for Game {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Slopcraft")
            .with_inner_size(LogicalSize::new(1280.0, 800.0))
            .with_min_inner_size(LogicalSize::new(580.0, 400.0));
        let result = event_loop
            .create_window(attrs)
            .map(Arc::new)
            .map_err(|error| error.to_string())
            .and_then(|window| {
                Renderer::new(window.clone(), generate_textures())
                    .map(|renderer| (window, renderer))
            });
        match result {
            Ok((window, renderer)) => {
                self.renderer = Some(renderer);
                self.update_center();
                self.stream(STARTUP_CHUNKS, Duration::MAX);
                if let Some(path) = self.initial_shader.take() {
                    match self.renderer.as_mut().unwrap().reload_shader(&path) {
                        Ok(name) => {
                            self.shader_name = name;
                            self.active_shader = Some(path.clone());
                            if let Some(index) =
                                self.shader_packs.iter().position(|pack| pack == &path)
                            {
                                self.shader_index = index + 1;
                            } else {
                                self.shader_packs.push(path);
                                self.shader_index = self.shader_packs.len();
                            }
                        }
                        Err(error) => {
                            eprintln!("Shader startup: {error}");
                            self.show_status("SHADER ERROR - USING DAYLIGHT");
                        }
                    }
                }
                window.request_redraw();
                self.window = Some(window);
            }
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self
            .window
            .as_ref()
            .is_none_or(|window| window.id() != window_id)
        {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                self.error = self.save(false).err();
                event_loop.exit();
            }
            WindowEvent::Focused(false) => self.set_paused(true),
            WindowEvent::Resized(size) => self
                .renderer
                .as_mut()
                .unwrap()
                .resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                self.redraw(event_loop);
                return;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let searching = self.inventory.open;
                if let PhysicalKey::Code(key) = event.physical_key {
                    self.keyboard(key, event.state, event.repeat);
                }
                if searching && self.inventory.open && event.state.is_pressed() {
                    if let Some(text) = event.text {
                        if !matches!(
                            event.physical_key,
                            PhysicalKey::Code(
                                KeyCode::Digit1
                                    | KeyCode::Digit2
                                    | KeyCode::Digit3
                                    | KeyCode::Digit4
                                    | KeyCode::Digit5
                                    | KeyCode::Digit6
                                    | KeyCode::Digit7
                            )
                        ) {
                            self.inventory.search(&text);
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32)
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if !state.is_pressed() {
                    match button {
                        MouseButton::Left => self.controls.breaking = false,
                        MouseButton::Right => self.controls.placing = false,
                        _ => (),
                    }
                    return;
                }
                if self.inventory.open {
                    if button == MouseButton::Left {
                        let size = self.window.as_ref().unwrap().inner_size();
                        if let Some(local) =
                            inventory_cell(size.width, size.height, self.cursor.0, self.cursor.1)
                        {
                            if let Some(entry) = self.inventory.choose_cell(local) {
                                self.equip(entry);
                            }
                        }
                    }
                } else if self.paused && button == MouseButton::Left {
                    self.set_paused(false);
                } else if !self.paused {
                    self.edit(button);
                    self.last_edit = Instant::now();
                    match button {
                        MouseButton::Left => self.controls.breaking = true,
                        MouseButton::Right => self.controls.placing = true,
                        _ => (),
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } if self.inventory.open => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(delta) => delta.y as f32,
                };
                if y == 0.0 {
                    return;
                }
                self.inventory.navigate(if y > 0.0 {
                    -(PAGE_SIZE as isize)
                } else {
                    PAGE_SIZE as isize
                });
            }
            WindowEvent::MouseWheel { delta, .. } if !self.paused => {
                self.controls.scroll += match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(delta) => (delta.y / 40.0) as f32,
                };
                let whole = self.controls.scroll.trunc();
                self.controls.scroll -= whole;
                self.selected =
                    (self.selected as i32 - whole as i32).rem_euclid(HOTBAR.len() as i32) as usize;
            }
            _ => return,
        }
        self.window.as_ref().unwrap().request_redraw();
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if self.paused {
            return;
        }
        if let DeviceEvent::MouseMotion { delta: (x, y) } = event {
            self.player.look(x, y);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        use winit::event_loop::ControlFlow;

        let Some(window) = self.window.clone() else {
            return;
        };
        if !self.paused || !self.pending.is_empty() {
            window.request_redraw();
            event_loop.set_control_flow(ControlFlow::Wait);
        } else {
            // Wake only for pending save work or to clear the temporary message.
            let now = Instant::now();
            if self.dirty && now.duration_since(self.last_save_attempt) >= AUTOSAVE_INTERVAL {
                let _ = self.save(false);
                window.request_redraw();
            }
            if !self.status.is_empty() && self.status_until <= now {
                self.status.clear();
                window.request_redraw();
            }
            let save_deadline = self
                .dirty
                .then_some(self.last_save_attempt + AUTOSAVE_INTERVAL);
            let status_deadline = (!self.status.is_empty()).then_some(self.status_until);
            let deadline = [save_deadline, status_deadline].into_iter().flatten().min();
            event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
        }
    }
}
