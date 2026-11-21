use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use font8x8::{BASIC_FONTS, UnicodeFonts};

use super::{Frame, InventoryView, texture_layout_entry};

const FONT_WIDTH: u32 = 128;
const FONT_HEIGHT: u32 = 64;
const FONT_ADVANCE: f32 = 8.0;
const INK: [f32; 4] = [0.055, 0.090, 0.105, 0.90];
const PALE: [f32; 4] = [0.93, 0.97, 0.93, 1.0];
const MINT: [f32; 4] = [0.65, 0.88, 0.72, 1.0];
const MUTED: [f32; 4] = [0.69, 0.76, 0.74, 1.0];
const WHITE: [f32; 4] = [1.0; 4];
const SOLID: u32 = 0;
const GLYPH: u32 = 1;
const TILE: u32 = 2;
const INVENTORY_COLUMNS: usize = 8;
const INVENTORY_ROWS: usize = 5;
const INVENTORY_WIDTH: f32 = 820.0;
const INVENTORY_HEIGHT: f32 = 558.0;
const CELL_STRIDE_X: f32 = 96.0;
const CELL_STRIDE_Y: f32 = 60.0;
const CELL_WIDTH: f32 = 90.0;
const CELL_HEIGHT: f32 = 56.0;

fn ui_scale(width: u32, height: u32) -> f32 {
    let width = width.max(1) as f32;
    let height = height.max(1) as f32;
    (width / 1080.0)
        .min(height / 700.0)
        .clamp(0.84, 2.0)
        .min(width / (INVENTORY_WIDTH + 24.0))
        .min(height / (INVENTORY_HEIGHT + 120.0))
}

fn inventory_panel(width: f32, height: f32) -> Rect {
    Rect::new(
        (width - INVENTORY_WIDTH) * 0.5,
        ((height - INVENTORY_HEIGHT) * 0.5 - 24.0).max(10.0),
        INVENTORY_WIDTH,
        INVENTORY_HEIGHT,
    )
}

pub(super) fn inventory_cell(width: u32, height: u32, x: f32, y: f32) -> Option<usize> {
    let scale = ui_scale(width, height);
    let panel = inventory_panel(width as f32 / scale, height as f32 / scale);
    let grid_x = x / scale - panel.x - 29.0;
    let grid_y = y / scale - panel.y - 108.0;
    if grid_x < 0.0 || grid_y < 0.0 {
        return None;
    }
    let column = (grid_x / CELL_STRIDE_X) as usize;
    let row = (grid_y / CELL_STRIDE_Y) as usize;
    if column >= INVENTORY_COLUMNS
        || row >= INVENTORY_ROWS
        || grid_x % CELL_STRIDE_X >= CELL_WIDTH
        || grid_y % CELL_STRIDE_Y >= CELL_HEIGHT
    {
        return None;
    }
    Some(row * INVENTORY_COLUMNS + column)
}

const UI_ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Uint32, 4 => Uint32];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UiVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    kind: u32,
    layer: u32,
}

pub(super) struct Hud {
    pipeline: wgpu::RenderPipeline,
    font_bindings: wgpu::BindGroup,
    buffer: wgpu::Buffer,
    buffer_capacity: u64,
    batch: Batch,
}

impl Hud {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        tile_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let (font_layout, font_bindings) = create_font(device, queue);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("HUD pipeline layout"),
            bind_group_layouts: &[tile_layout, &font_layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("hud.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("HUD pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("hud_vertex"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: size_of::<UiVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &UI_ATTRIBUTES,
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("hud_fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let buffer_capacity = 64 * 1024;
        let buffer = create_vertex_buffer(device, buffer_capacity);
        Self {
            pipeline,
            font_bindings,
            buffer,
            buffer_capacity,
            batch: Batch::default(),
        }
    }

    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &Frame<'_>,
        width: u32,
        height: u32,
    ) {
        let screen_width = width as f32;
        let screen_height = height as f32;
        let scale = ui_scale(width, height);
        self.batch.reset(screen_width, screen_height, scale);
        if let Some(inventory) = &frame.inventory {
            self.batch.hotbar(frame);
            self.batch.inventory(inventory, frame.selected);
        } else if frame.paused {
            self.batch.pause(frame.environment);
        } else {
            self.batch.crosshair(frame.target.is_some());
            self.batch.hotbar(frame);
            self.batch.environment(frame.environment);
        }
        self.batch
            .status(frame.status, frame.paused, frame.inventory.is_some());
        self.batch.debug(frame.debug);

        let bytes = bytemuck::cast_slice(&self.batch.vertices);
        if bytes.len() as u64 > self.buffer_capacity {
            self.buffer_capacity = (bytes.len() as u64).next_power_of_two();
            self.buffer = create_vertex_buffer(device, self.buffer_capacity);
        }
        queue.write_buffer(&self.buffer, 0, bytes);
    }

    pub(super) fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        tiles: &wgpu::BindGroup,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("HUD"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, tiles, &[]);
        pass.set_bind_group(1, &self.font_bindings, &[]);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..self.batch.vertices.len() as u32, 0..1);
    }
}

fn create_vertex_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("HUD vertices"),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_font(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
    let mut glyphs = vec![0; (FONT_WIDTH * FONT_HEIGHT) as usize];
    for ascii in 32..=126_u8 {
        if let Some(rows) = BASIC_FONTS.get(ascii as char) {
            let tile_x = usize::from(ascii % 16) * 8;
            let tile_y = usize::from(ascii / 16) * 8;
            for (y, row) in rows.into_iter().enumerate() {
                for x in 0..8 {
                    let pixel = (tile_y + y) * FONT_WIDTH as usize + tile_x + x;
                    glyphs[pixel] = ((row >> x) & 1) * 255;
                }
            }
        }
    }
    let size = wgpu::Extent3d {
        width: FONT_WIDTH,
        height: FONT_HEIGHT,
        depth_or_array_layers: 1,
    };
    let font = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("8x8 HUD glyphs"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &font,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &glyphs,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(FONT_WIDTH),
            rows_per_image: None,
        },
        size,
    );
    let view = font.create_view(&wgpu::TextureViewDescriptor::default());
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("glyph texture layout"),
        entries: &[texture_layout_entry(0, wgpu::TextureViewDimension::D2)],
    });
    let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("glyph texture bindings"),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });
    (layout, bindings)
}

#[derive(Default)]
struct Batch {
    width: f32,
    height: f32,
    pixel_scale: f32,
    vertices: Vec<UiVertex>,
}

#[derive(Clone, Copy)]
struct Rect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Rect {
    fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn inset(self, inset: f32) -> Self {
        Self::new(
            self.x + inset,
            self.y + inset,
            self.width - 2.0 * inset,
            self.height - 2.0 * inset,
        )
    }

    fn corners(self) -> [[f32; 2]; 4] {
        let right = self.x + self.width;
        let bottom = self.y + self.height;
        [
            [self.x, self.y],
            [right, self.y],
            [right, bottom],
            [self.x, bottom],
        ]
    }
}

impl Batch {
    fn reset(&mut self, width: f32, height: f32, pixel_scale: f32) {
        self.width = width / pixel_scale;
        self.height = height / pixel_scale;
        self.pixel_scale = pixel_scale;
        self.vertices.clear();
    }

    fn quad(
        &mut self,
        corners: [[f32; 2]; 4],
        uv: [[f32; 2]; 4],
        color: [f32; 4],
        kind: u32,
        layer: u32,
    ) {
        for index in [0, 1, 2, 0, 2, 3] {
            let [x, y] = corners[index];
            self.vertices.push(UiVertex {
                position: [2.0 * x / self.width - 1.0, 1.0 - 2.0 * y / self.height],
                uv: uv[index],
                color,
                kind,
                layer,
            });
        }
    }

    fn rect(&mut self, rect: Rect, color: [f32; 4]) {
        self.quad(rect.corners(), [[0.0; 2]; 4], color, SOLID, 0);
    }

    fn text(&mut self, x: f32, y: f32, value: &str, zoom: f32, color: [f32; 4]) {
        let x = (x * self.pixel_scale).round() / self.pixel_scale;
        let y = (y * self.pixel_scale).round() / self.pixel_scale;
        for (index, char) in value.chars().enumerate() {
            if char == ' ' {
                continue;
            }
            let byte = if char.is_ascii_graphic() {
                char as u8
            } else {
                b'?'
            };
            let glyph_size = FONT_ADVANCE * zoom;
            let glyph_x = f32::from(byte % 16) * 8.0;
            let glyph_y = f32::from(byte / 16) * 8.0;
            self.quad(
                Rect::new(x + index as f32 * glyph_size, y, glyph_size, glyph_size).corners(),
                Rect::new(glyph_x, glyph_y, 8.0, 8.0).corners(),
                color,
                GLYPH,
                0,
            );
        }
    }

    fn centered(&mut self, y: f32, value: &str, zoom: f32, color: [f32; 4]) {
        let x = (self.width - text_width(value, zoom)) * 0.5;
        self.text(x, y, value, zoom, color);
    }

    fn fitted_text(&mut self, x: f32, y: f32, value: &str, zoom: f32, width: f32, color: [f32; 4]) {
        let capacity = (width / (FONT_ADVANCE * zoom)) as usize;
        if value.chars().count() <= capacity {
            self.text(x, y, value, zoom, color);
        } else {
            let short: String = value.chars().take(capacity.saturating_sub(2)).collect();
            self.text(x, y, &format!("{short}.."), zoom, color);
        }
    }

    fn crosshair(&mut self, on_block: bool) {
        let x = self.width / 2.0;
        let y = self.height / 2.0;
        let color = if on_block {
            PALE
        } else {
            [0.96, 0.98, 1.0, 0.86]
        };
        let bars = [
            Rect::new(x - 8.0, y - 0.75, 5.0, 1.5),
            Rect::new(x + 3.0, y - 0.75, 5.0, 1.5),
            Rect::new(x - 0.75, y - 8.0, 1.5, 5.0),
            Rect::new(x - 0.75, y + 3.0, 1.5, 5.0),
        ];
        for bar in bars {
            self.rect(bar.inset(-0.85), INK);
            self.rect(bar, color);
        }
    }

    fn block_icon(&mut self, x: f32, y: f32, layer: u32) {
        let top = [x, y];
        let left = [x - 12.0, y + 7.0];
        let right = [x + 12.0, y + 7.0];
        let center = [x, y + 14.0];
        let bottom = [x, y + 28.0];
        let uv = Rect::new(0.0, 0.0, 1.0, 1.0).corners();
        self.quad(
            [left, center, bottom, [left[0], left[1] + 14.0]],
            uv,
            [0.82, 0.85, 0.84, 1.0],
            TILE,
            layer,
        );
        self.quad(
            [center, right, [right[0], right[1] + 14.0], bottom],
            uv,
            [0.68, 0.73, 0.74, 1.0],
            TILE,
            layer,
        );
        self.quad([top, right, center, left], uv, WHITE, TILE, layer);
    }

    fn item_icon(&mut self, x: f32, y: f32, layer: u32) {
        let rect = Rect::new(x - 14.0, y, 28.0, 28.0);
        let uv = Rect::new(0.0, 0.0, 1.0, 1.0).corners();
        self.quad(rect.corners(), uv, WHITE, TILE, layer);
    }

    fn hotbar(&mut self, frame: &Frame<'_>) {
        let stride = 48.0;
        let outer_width = frame.slots.len() as f32 * stride + 10.0;
        let x = (self.width - outer_width) * 0.5;
        let y = self.height - 75.0;
        self.rect(Rect::new(x, y, outer_width, 54.0), INK);

        for (index, slot) in frame.slots.iter().enumerate() {
            let card = Rect::new(x + 7.0 + stride * index as f32, y + 5.0, 44.0, 44.0);
            let selected = index == frame.selected;
            if selected {
                self.rect(card, MINT);
                self.rect(card.inset(2.0), [0.17, 0.27, 0.24, 1.0]);
            } else {
                self.rect(card, [0.45, 0.57, 0.56, 0.26]);
                self.rect(card.inset(1.0), [0.085, 0.14, 0.15, 0.93]);
            }
            if slot.item {
                self.item_icon(card.x + 22.0, card.y + 8.0, slot.layer);
            } else {
                self.block_icon(card.x + 22.0, card.y + 8.0, slot.layer);
            }
            if let Some(key) = ["1", "2", "3", "4", "5", "6", "7"].get(index) {
                let color = if selected { PALE } else { MUTED };
                self.text(card.x + 4.0, card.y + 4.0, key, 1.0, color);
            }
        }

        if let Some(slot) = frame.slots.get(frame.selected) {
            let width = text_width(slot.name, 1.5) + 22.0;
            self.rect(
                Rect::new((self.width - width) * 0.5, y - 34.0, width, 24.0),
                INK,
            );
            self.centered(y - 27.0, slot.name, 1.5, PALE);
        }
    }

    fn inventory(&mut self, view: &InventoryView<'_>, hotbar_slot: usize) {
        self.rect(
            Rect::new(0.0, 0.0, self.width, self.height),
            [0.025, 0.055, 0.07, 0.66],
        );
        let panel = inventory_panel(self.width, self.height);
        self.rect(
            Rect::new(panel.x + 5.0, panel.y + 8.0, panel.width, panel.height),
            [0.02, 0.04, 0.05, 0.46],
        );
        self.rect(panel, [0.42, 0.58, 0.56, 0.94]);
        self.rect(panel.inset(1.0), [0.075, 0.125, 0.14, 0.98]);
        self.rect(Rect::new(panel.x + 28.0, panel.y, 106.0, 3.0), MINT);
        self.fitted_text(panel.x + 29.0, panel.y + 25.0, view.title, 2.4, 570.0, PALE);
        let page = format!("PAGE {} / {}", view.page + 1, view.pages.max(1));
        self.text(
            panel.x + panel.width - text_width(&page, 1.25) - 29.0,
            panel.y + 31.0,
            &page,
            1.25,
            MUTED,
        );

        let search = Rect::new(panel.x + 29.0, panel.y + 62.0, 762.0, 34.0);
        self.rect(search, [0.29, 0.44, 0.42, 1.0]);
        self.rect(search.inset(1.0), [0.045, 0.09, 0.105, 1.0]);
        let search_text = if view.query.is_empty() {
            "Type to search...".to_owned()
        } else {
            format!("{}|", view.query)
        };
        self.fitted_text(
            search.x + 12.0,
            search.y + 11.0,
            &search_text,
            1.45,
            search.width - 24.0,
            if view.query.is_empty() { MUTED } else { PALE },
        );

        for (index, entry) in view
            .entries
            .iter()
            .take(INVENTORY_COLUMNS * INVENTORY_ROWS)
            .enumerate()
        {
            let column = index % INVENTORY_COLUMNS;
            let row = index / INVENTORY_COLUMNS;
            let card = Rect::new(
                panel.x + 29.0 + column as f32 * CELL_STRIDE_X,
                panel.y + 108.0 + row as f32 * CELL_STRIDE_Y,
                CELL_WIDTH,
                CELL_HEIGHT,
            );
            let selected = index == view.selected;
            self.rect(
                card,
                if selected {
                    MINT
                } else {
                    [0.25, 0.36, 0.37, 1.0]
                },
            );
            self.rect(
                card.inset(if selected { 2.0 } else { 1.0 }),
                if selected {
                    [0.17, 0.27, 0.24, 1.0]
                } else {
                    [0.105, 0.17, 0.19, 1.0]
                },
            );
            if entry.item {
                self.item_icon(card.x + CELL_WIDTH * 0.5, card.y + 5.0, entry.layer);
            } else {
                self.block_icon(card.x + CELL_WIDTH * 0.5, card.y + 5.0, entry.layer);
            }
            let caption: String = entry.name.chars().take(11).collect();
            let zoom = 0.9;
            self.text(
                card.x + (card.width - text_width(&caption, zoom)) * 0.5,
                card.y + 42.0,
                &caption,
                zoom,
                if selected { PALE } else { MUTED },
            );
        }

        if let Some(entry) = view.entries.get(view.selected) {
            self.fitted_text(
                panel.x + 29.0,
                panel.y + 421.0,
                entry.name,
                1.7,
                580.0,
                PALE,
            );
            self.fitted_text(
                panel.x + 29.0,
                panel.y + 446.0,
                entry.category,
                1.25,
                580.0,
                MINT,
            );
        } else {
            self.text(
                panel.x + 29.0,
                panel.y + 421.0,
                "No matching entries",
                1.7,
                PALE,
            );
        }
        let equip = format!("HOTBAR {}", hotbar_slot + 1);
        self.text(
            panel.x + panel.width - text_width(&equip, 1.25) - 29.0,
            panel.y + 445.0,
            &equip,
            1.25,
            MUTED,
        );
        self.rect(
            Rect::new(panel.x + 29.0, panel.y + 475.0, 762.0, 1.0),
            [0.38, 0.49, 0.48, 0.7],
        );
        self.text(
            panel.x + 29.0,
            panel.y + 492.0,
            "Click equip   ENTER equip & close   ESC close",
            1.3,
            PALE,
        );
        self.text(
            panel.x + 29.0,
            panel.y + 519.0,
            "TAB blocks/items   ARROWS select   PGUP/PGDN page   1-7 slot",
            1.2,
            MUTED,
        );
    }

    fn environment(&mut self, environment: &str) {
        if environment.is_empty() {
            return;
        }
        let value = environment.lines().next().unwrap_or_default();
        let size = 1.1;
        let width = (text_width(value, size) + 22.0).min(self.width - 30.0);
        self.rect(
            Rect::new(15.0, 15.0, width, 25.0),
            [INK[0], INK[1], INK[2], 0.76],
        );
        self.fitted_text(26.0, 23.0, value, size, width - 22.0, MUTED);
    }

    fn pause(&mut self, environment: &str) {
        self.rect(
            Rect::new(0.0, 0.0, self.width, self.height),
            [0.04, 0.08, 0.10, 0.55],
        );
        let panel_x = (self.width - 472.0) * 0.5;
        let panel_y = (self.height - 336.0) * 0.5;
        self.rect(
            Rect::new(panel_x + 5.0, panel_y + 8.0, 472.0, 336.0),
            [0.025, 0.05, 0.06, 0.44],
        );
        self.rect(
            Rect::new(panel_x, panel_y, 472.0, 336.0),
            [0.42, 0.58, 0.56, 0.83],
        );
        self.rect(
            Rect::new(panel_x + 1.0, panel_y + 1.0, 470.0, 334.0),
            [0.085, 0.140, 0.150, 0.96],
        );
        self.rect(Rect::new(panel_x + 186.0, panel_y, 100.0, 3.0), MINT);
        self.centered(panel_y + 38.0, "SLOPCRAFT", 4.7, PALE);

        let button_x = (self.width - 257.0) * 0.5;
        self.rect(Rect::new(button_x, panel_y + 105.0, 257.0, 48.0), MINT);
        self.rect(
            Rect::new(button_x, panel_y + 149.0, 257.0, 4.0),
            [0.42, 0.68, 0.54, 1.0],
        );
        self.centered(
            panel_y + 119.0,
            "CLICK TO PLAY",
            2.05,
            [0.065, 0.135, 0.12, 1.0],
        );

        self.rect(
            Rect::new(panel_x + 32.0, panel_y + 177.0, 408.0, 1.0),
            [0.43, 0.55, 0.56, 0.48],
        );
        self.centered(
            panel_y + 193.0,
            "WASD Move   SPACE Jump   SHIFT Sprint",
            1.22,
            PALE,
        );
        self.centered(
            panel_y + 218.0,
            "LMB Break   RMB Place   1-7 / WHEEL Select",
            1.22,
            PALE,
        );
        self.centered(
            panel_y + 243.0,
            "E Inventory   F5 Save   F3 Stats",
            1.22,
            MUTED,
        );
        self.centered(
            panel_y + 268.0,
            "F6 Reload shader   F7 Change shader",
            1.22,
            MUTED,
        );
        self.centered(panel_y + 293.0, "ESC Pause / release mouse", 1.22, MUTED);
        if !environment.is_empty() {
            self.environment(environment);
        }
    }

    fn status(&mut self, status: &str, paused: bool, inventory: bool) {
        if status.is_empty() {
            return;
        }
        let line = status.lines().next().unwrap_or_default();
        let size = 1.45_f32.min((self.width - 48.0).max(0.0) / text_width(line, 1.0).max(1.0));
        let width = text_width(line, size) + 26.0;
        let y = if inventory {
            (inventory_panel(self.width, self.height).y - 29.0).max(3.0)
        } else if paused {
            self.height / 2.0 + 180.0
        } else {
            self.height - 140.0
        };
        self.rect(Rect::new((self.width - width) * 0.5, y, width, 24.0), INK);
        self.centered(y + 7.0, line, size, PALE);
    }

    fn debug(&mut self, debug: &str) {
        if debug.is_empty() {
            return;
        }
        let width = debug
            .lines()
            .map(|line| text_width(line, 1.25))
            .fold(0.0, f32::max)
            + 20.0;
        let height = debug.lines().count() as f32 * 16.0 + 12.0;
        self.rect(Rect::new(15.0, 15.0, width, height), INK);
        for (index, line) in debug.lines().enumerate() {
            self.text(25.0, 22.0 + index as f32 * 16.0, line, 1.25, PALE);
        }
    }
}

fn text_width(value: &str, zoom: f32) -> f32 {
    value.chars().count() as f32 * FONT_ADVANCE * zoom
}
