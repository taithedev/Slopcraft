mod hud;

use std::{collections::HashMap, mem::size_of, path::Path, sync::Arc};

use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::{
    mesh::{ChunkMesh, TexturePack, Vertex},
    world::ChunkPos,
};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

const TERRAIN_ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Uint32, 4 => Float32];

#[derive(Clone, Copy)]
pub struct Slot<'a> {
    pub name: &'a str,
    pub layer: u32,
    pub item: bool,
}

pub struct CatalogSlot<'a> {
    pub name: &'a str,
    pub category: &'a str,
    pub layer: u32,
    pub item: bool,
}

pub struct InventoryView<'a> {
    pub title: &'a str,
    pub query: &'a str,
    pub entries: &'a [CatalogSlot<'a>],
    pub page: usize,
    pub pages: usize,
    pub selected: usize,
}

pub struct Frame<'a> {
    pub eye: Vec3,
    pub direction: Vec3,
    pub selected: usize,
    pub slots: &'a [Slot<'a>],
    pub paused: bool,
    pub status: &'a str,
    pub target: Option<IVec3>,
    pub debug: &'a str,
    pub environment: &'a str,
    pub view_distance: f32,
    pub inventory: Option<InventoryView<'a>>,
}

/// Returns the page-local index of the inventory cell under a physical pixel.
pub fn inventory_cell(width: u32, height: u32, x: f32, y: f32) -> Option<usize> {
    hud::inventory_cell(width, height, x, y)
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_projection: [[f32; 4]; 4],
    inverse_view_projection: [[f32; 4]; 4],
    eye: [f32; 4],
    target_block: [f32; 4],
    atmosphere: [f32; 4],
}

struct ChunkBuffers {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    minimum: Vec3,
    maximum: Vec3,
}

// Terrain shaders share this texture-array binding. Materials refer to stable
// registry-assigned layers, while chunk meshes carry no GPU material objects.
struct BlockMaterial {
    layout: wgpu::BindGroupLayout,
    bindings: wgpu::BindGroup,
}

impl BlockMaterial {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, mut pack: TexturePack) -> Self {
        let mip_count = pack.size.ilog2() + 1;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("original block tile array"),
            size: wgpu::Extent3d {
                width: pack.size,
                height: pack.size,
                depth_or_array_layers: pack.layers,
            },
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        for level in 0..mip_count {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &pack.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pack.size * 4),
                    rows_per_image: Some(pack.size),
                },
                wgpu::Extent3d {
                    width: pack.size,
                    height: pack.size,
                    depth_or_array_layers: pack.layers,
                },
            );
            if level + 1 < mip_count {
                downsample_tiles(&mut pack);
            }
        }

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("tile repeat sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("block material layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture_layout_entry(1, wgpu::TextureViewDimension::D2Array),
            ],
        });
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("block material bindings"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        });
        Self { layout, bindings }
    }
}

fn downsample_tiles(pack: &mut TexturePack) {
    let size = pack.size as usize;
    let next_size = size / 2;
    let mut values = Vec::with_capacity(next_size * next_size * pack.layers as usize * 4);
    for layer in pack.rgba.chunks_exact(size * size * 4) {
        for y in 0..next_size {
            for x in 0..next_size {
                let first = (y * 2 * size + x * 2) * 4;
                for channel in 0..4 {
                    let offset = first + channel;
                    let value = u16::from(layer[offset])
                        + u16::from(layer[offset + 4])
                        + u16::from(layer[offset + 4 * size])
                        + u16::from(layer[offset + 4 * size + 4]);
                    values.push(value.div_ceil(4) as u8);
                }
            }
        }
    }
    pack.size = next_size as u32;
    pack.rgba = values;
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    suspended: bool,
    camera_buffer: wgpu::Buffer,
    camera_bindings: wgpu::BindGroup,
    block_material: BlockMaterial,
    terrain_pipeline: wgpu::RenderPipeline,
    sky_pipeline: wgpu::RenderPipeline,
    outline_pipeline: wgpu::RenderPipeline,
    terrain_layout: wgpu::PipelineLayout,
    world_layout: wgpu::PipelineLayout,
    chunks: HashMap<ChunkPos, ChunkBuffers>,
    hud: hud::Hud,
}

impl Renderer {
    pub fn new(window: Arc<Window>, textures: TexturePack) -> Result<Self, String> {
        pollster::block_on(Self::initialize(window, textures))
    }

    async fn initialize(window: Arc<Window>, textures: TexturePack) -> Result<Self, String> {
        let size = window.inner_size();
        let mut instance_desc = wgpu::InstanceDescriptor::from_env_or_default();
        if cfg!(target_os = "windows") {
            instance_desc.backends &= wgpu::Backends::DX12 | wgpu::Backends::VULKAN;
        }
        let instance = wgpu::Instance::new(&instance_desc);
        let surface = instance
            .create_surface(window)
            .map_err(|error| format!("Couldn't create the game surface: {error}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "Couldn't find a compatible graphics adapter".to_owned())?;
        let supported_layers = adapter.limits().max_texture_array_layers;
        if supported_layers < 512 {
            return Err(format!(
                "Slopcraft needs 512 texture array layers. This graphics adapter supports {supported_layers}."
            ));
        }
        if textures.layers > 512 {
            return Err(format!(
                "The material registry has {} layers, exceeding the 512-layer budget",
                textures.layers
            ));
        }
        let limits = wgpu::Limits {
            max_texture_array_layers: 512,
            ..Default::default()
        };
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Slopcraft graphics device"),
                    required_limits: limits,
                    ..Default::default()
                },
                None,
            )
            .await
            .map_err(|error| format!("Couldn't initialize graphics: {error}"))?;
        let info = adapter.get_info();
        eprintln!(
            "Slopcraft graphics: {} ({:?}); 512 texture layers",
            info.name, info.backend
        );

        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| "The graphics adapter cannot present to this window".to_owned())?;
        let capabilities = surface.get_capabilities(&adapter);
        if let Some(format) = capabilities
            .formats
            .into_iter()
            .find(|format| format.is_srgb())
        {
            config.format = format;
        }
        surface.configure(&device, &config);
        let depth = create_depth(&device, &config);

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera uniform"),
            size: size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera bindings"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let block_material = BlockMaterial::new(&device, &queue, textures);

        let terrain_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain pipeline layout"),
            bind_group_layouts: &[&camera_layout, &block_material.layout],
            push_constant_ranges: &[],
        });
        let world_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky and outline pipeline layout"),
            bind_group_layouts: &[&camera_layout],
            push_constant_ranges: &[],
        });
        let world_shader = device.create_shader_module(wgpu::include_wgsl!("terrain.wgsl"));
        let terrain_pipeline = world_pipeline(
            &device,
            &terrain_layout,
            &world_shader,
            config.format,
            WorldPass::Terrain,
        );
        let sky_pipeline = world_pipeline(
            &device,
            &world_layout,
            &world_shader,
            config.format,
            WorldPass::Sky,
        );
        let outline_pipeline = world_pipeline(
            &device,
            &world_layout,
            &world_shader,
            config.format,
            WorldPass::Outline,
        );
        let hud = hud::Hud::new(&device, &queue, config.format, &block_material.layout);
        if let Some(error) = device.pop_error_scope().await {
            return Err(format!("Renderer initialization failed: {error}"));
        }

        Ok(Self {
            surface,
            device,
            queue,
            config,
            depth,
            suspended: size.width == 0 || size.height == 0,
            camera_buffer,
            camera_bindings,
            block_material,
            terrain_pipeline,
            sky_pipeline,
            outline_pipeline,
            terrain_layout,
            world_layout,
            chunks: HashMap::new(),
            hud,
        })
    }

    /// Compile and validate every world pipeline before changing the active set.
    pub fn use_shader(&mut self, source: &str) -> Result<(), String> {
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("external world shader"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        if let Some(error) = pollster::block_on(self.device.pop_error_scope()) {
            return Err(format!("Shader source rejected: {error}"));
        }

        self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let terrain = world_pipeline(
            &self.device,
            &self.terrain_layout,
            &module,
            self.config.format,
            WorldPass::Terrain,
        );
        let sky = world_pipeline(
            &self.device,
            &self.world_layout,
            &module,
            self.config.format,
            WorldPass::Sky,
        );
        let outline = world_pipeline(
            &self.device,
            &self.world_layout,
            &module,
            self.config.format,
            WorldPass::Outline,
        );
        if let Some(error) = pollster::block_on(self.device.pop_error_scope()) {
            return Err(format!("Shader pipeline rejected: {error}"));
        }

        self.terrain_pipeline = terrain;
        self.sky_pipeline = sky;
        self.outline_pipeline = outline;
        Ok(())
    }

    /// Read a pack directory or a WGSL file. Failed reloads retain the active set.
    pub fn reload_shader(&mut self, path: &Path) -> Result<String, String> {
        let is_pack = path.is_dir();
        let name = if is_pack {
            path.file_name()
        } else {
            path.file_stem()
        }
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Custom".to_owned());
        let path = if is_pack {
            path.join("terrain.wgsl")
        } else {
            path.to_owned()
        };
        let size = std::fs::metadata(&path)
            .map_err(|error| format!("Reading {} failed: {error}", path.display()))?
            .len();
        if size > 2 * 1024 * 1024 {
            return Err("Shader packs may contain at most 2 MiB of WGSL source".to_owned());
        }
        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("Reading {} failed: {error}", path.display()))?;
        self.use_shader(&source)?;
        Ok(name)
    }

    pub fn use_builtin_shader(&mut self) -> Result<(), String> {
        self.use_shader(include_str!("terrain.wgsl"))
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.suspended = width == 0 || height == 0;
        if !self.suspended {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.depth = create_depth(&self.device, &self.config);
        }
    }

    pub fn upload(&mut self, pos: ChunkPos, mesh: ChunkMesh) {
        if mesh.indices.is_empty() {
            self.remove(pos);
            return;
        }
        let mut minimum = Vec3::splat(f32::INFINITY);
        let mut maximum = Vec3::splat(f32::NEG_INFINITY);
        for vertex in &mesh.vertices {
            let point = Vec3::from_array(vertex.position);
            minimum = minimum.min(point);
            maximum = maximum.max(point);
        }
        let vertices = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("chunk vertices"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let indices = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("chunk indices"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        self.chunks.insert(
            pos,
            ChunkBuffers {
                vertices,
                indices,
                index_count: mesh.indices.len() as u32,
                minimum,
                maximum,
            },
        );
    }

    pub fn remove(&mut self, pos: ChunkPos) {
        self.chunks.remove(&pos);
    }

    pub fn draw(&mut self, frame: &Frame<'_>) -> Result<(), wgpu::SurfaceError> {
        if self.suspended {
            return Ok(());
        }
        let output = self.surface.get_current_texture()?;
        let output_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let view = Mat4::look_to_rh(frame.eye, frame.direction, Vec3::Y);
        let projection = Mat4::perspective_rh(
            65.0_f32.to_radians(),
            self.config.width as f32 / self.config.height as f32,
            0.06,
            (frame.view_distance * 2.0).max(104.0),
        );
        let view_projection = projection * view;
        let camera = CameraUniform {
            view_projection: view_projection.to_cols_array_2d(),
            inverse_view_projection: view_projection.inverse().to_cols_array_2d(),
            eye: frame.eye.extend(1.0).to_array(),
            target_block: frame
                .target
                .unwrap_or(IVec3::ZERO)
                .as_vec3()
                .extend(1.0)
                .to_array(),
            atmosphere: [
                frame.view_distance.max(36.0) * 0.51,
                (frame.view_distance - 10.0).max(30.0),
                0.0,
                0.0,
            ],
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera));
        self.hud.prepare(
            &self.device,
            &self.queue,
            frame,
            self.config.width,
            self.config.height,
        );
        let frustum = Frustum::new(view_projection);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Slopcraft frame"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("voxel world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.terrain_pipeline);
            pass.set_bind_group(0, &self.camera_bindings, &[]);
            pass.set_bind_group(1, &self.block_material.bindings, &[]);
            for chunk in self.chunks.values() {
                if frustum.intersects(chunk.minimum, chunk.maximum) {
                    pass.set_vertex_buffer(0, chunk.vertices.slice(..));
                    pass.set_index_buffer(chunk.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..chunk.index_count, 0, 0..1);
                }
            }
            // Far-plane depth testing skips sky fragments behind the terrain.
            pass.set_pipeline(&self.sky_pipeline);
            pass.draw(0..3, 0..1);
            if frame.target.is_some() && !frame.paused && frame.inventory.is_none() {
                pass.set_pipeline(&self.outline_pipeline);
                pass.draw(0..24, 0..1);
            }
        }

        self.hud
            .draw(&mut encoder, &output_view, &self.block_material.bindings);
        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }
}

fn create_depth(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("world depth buffer"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

enum WorldPass {
    Terrain,
    Sky,
    Outline,
}

fn world_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    pass: WorldPass,
) -> wgpu::RenderPipeline {
    let (vertex_entry, fragment_entry, topology, compare, cull_mode, blend) = match pass {
        WorldPass::Terrain => (
            "terrain_vertex",
            "terrain_fragment",
            wgpu::PrimitiveTopology::TriangleList,
            wgpu::CompareFunction::Less,
            Some(wgpu::Face::Back),
            None,
        ),
        WorldPass::Sky => (
            "sky_vertex",
            "sky_fragment",
            wgpu::PrimitiveTopology::TriangleList,
            wgpu::CompareFunction::Equal,
            None,
            None,
        ),
        WorldPass::Outline => (
            "outline_vertex",
            "outline_fragment",
            wgpu::PrimitiveTopology::LineList,
            wgpu::CompareFunction::LessEqual,
            None,
            Some(wgpu::BlendState::ALPHA_BLENDING),
        ),
    };
    let terrain_attributes = [wgpu::VertexBufferLayout {
        array_stride: size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &TERRAIN_ATTRIBUTES,
    }];
    let is_terrain = matches!(pass, WorldPass::Terrain);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(vertex_entry),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex_entry),
            compilation_options: Default::default(),
            buffers: if is_terrain { &terrain_attributes } else { &[] },
        },
        primitive: wgpu::PrimitiveState {
            topology,
            cull_mode,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: is_terrain,
            depth_compare: compare,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

fn texture_layout_entry(
    binding: u32,
    view_dimension: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension,
            multisampled: false,
        },
        count: None,
    }
}

struct Frustum([Vec4; 6]);

impl Frustum {
    fn new(view_projection: Mat4) -> Self {
        let rows = view_projection.transpose();
        let (x, y, z, w) = (rows.x_axis, rows.y_axis, rows.z_axis, rows.w_axis);
        // WebGPU clip space uses -w..w for x/y and 0..w for z.
        Self([w + x, w - x, w + y, w - y, z, w - z])
    }

    fn intersects(&self, minimum: Vec3, maximum: Vec3) -> bool {
        self.0.iter().all(|plane| {
            let normal = plane.truncate();
            let far_corner = Vec3::select(normal.cmpge(Vec3::ZERO), maximum, minimum);
            normal.dot(far_corner) + plane.w >= 0.0
        })
    }
}
