//! WGPU renders offscreen; the Kit snapshot's image API is the presentation bridge.
//! Images are regenerated only for viewport / slice / camera changes, never per frame.
use crate::{
    color::{Oklch, max_chroma},
    model::{Plane, Project},
};
use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    slices: wgpu::ComputePipeline,
    surface: wgpu::RenderPipeline,
    mesh: wgpu::Buffer,
    vertices: u32,
    pub adapter_name: String,
}
pub fn world(c: Oklch) -> Vec3 {
    Vec3::new(
        (crate::model::normalized(c.h, 2) * 1.8 - 0.9) as f32,
        (c.c / 0.4 * 1.2 - 0.3) as f32,
        (c.l * 1.3 - 0.65) as f32,
    )
}
pub fn camera(p: &Project, aspect: f32) -> Mat4 {
    let distance = 3.0 / p.zoom.clamp(0.5, 2.5);
    let eye = Vec3::new(
        distance * p.yaw.sin() * p.pitch.cos(),
        distance * p.pitch.sin(),
        distance * p.yaw.cos() * p.pitch.cos(),
    );
    Mat4::perspective_rh(40f32.to_radians(), aspect.max(0.1), 0.05, 50.)
        * Mat4::look_at_rh(eye, Vec3::new(0., 0.15, 0.), Vec3::Y)
}
pub fn project3(c: Oklch, p: &Project, aspect: f32) -> Option<[f64; 2]> {
    let v = camera(p, aspect) * world(c).extend(1.);
    if v.w <= 0.01 {
        return None;
    }
    Some([
        (v.x / v.w * 0.5 + 0.5) as f64,
        (v.y / v.w * 0.5 + 0.5) as f64,
    ])
}
impl Gpu {
    pub fn new() -> Result<Self> {
        pollster::block_on(Self::init())
    }
    async fn init() -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await
            .context("No WGPU graphics adapter")?;
        let adapter_name = adapter.get_info().name;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("OKLCH plot renderer"),
                ..Default::default()
            })
            .await?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("OKLCH slices"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/slices.wgsl").into()),
        });
        let slices = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Color space slices"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Gamut surface"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/gamut.wgsl").into()),
        });
        let surface = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Maximum chroma surface"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let mut data = Vec::new();
        let nh = 180;
        let nl = 80;
        let vertex = |h: usize, l: usize| {
            let light = l as f64 / nl as f64;
            let hue = h as f64 / nh as f64 * 360.;
            let chroma = max_chroma(light, hue);
            let c = Oklch::new(light, chroma, hue);
            Vertex {
                position: [
                    (hue / 360. * 1.8 - 0.9) as f32,
                    (chroma / 0.4 * 1.2 - 0.3) as f32,
                    (light * 1.3 - 0.65) as f32,
                ],
                color: c.srgb(false).map(|x| x as f32),
            }
        };
        for h in 0..nh {
            for l in 0..nl {
                let a = vertex(h, l);
                let b = vertex(h + 1, l);
                let c = vertex(h, l + 1);
                let d = vertex(h + 1, l + 1);
                data.extend([a, b, c, c, b, d]);
            }
        }
        // Close both hue edges down to the achromatic floor.
        for h in [0, nh] {
            for l in 0..nl {
                let a = vertex(h, l);
                let b = vertex(h, l + 1);
                let mut c = a;
                let mut d = b;
                c.position[1] = -0.3;
                d.position[1] = -0.3;
                c.color = Oklch::new(l as f64 / nl as f64, 0., 0.)
                    .srgb(false)
                    .map(|x| x as f32);
                d.color = Oklch::new((l + 1) as f64 / nl as f64, 0., 0.)
                    .srgb(false)
                    .map(|x| x as f32);
                data.extend([a, b, c, c, b, d]);
            }
        }
        let vertices = data.len() as u32;
        let mesh = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sRGB gamut mesh"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::VERTEX,
        });
        Ok(Self {
            device,
            queue,
            slices,
            surface,
            mesh,
            vertices,
            adapter_name,
        })
    }
    pub fn render(
        &self,
        plane: Option<Plane>,
        p: &Project,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>> {
        let width = width.clamp(64, 1200);
        let height = height.clamp(64, 1000);
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let output = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Plot image"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = output.create_view(&Default::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Plot commands"),
            });
        if let Some(plane) = plane {
            let plane = match plane {
                Plane::Cl => 0.,
                Plane::Ch => 1.,
                Plane::Lh => 2.,
            };
            let data = [
                width as f32,
                height as f32,
                plane,
                0.,
                p.slices[0] as f32,
                p.slices[1] as f32,
                p.slices[2] as f32,
                0.,
            ];
            let uniform = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&data),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.slices.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                ],
            });
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.slices);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        } else {
            let matrix = camera(p, width as f32 / height as f32).to_cols_array();
            let uniform = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&matrix),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.surface.get_bind_group_layout(0),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                }],
            });
            let depth = self.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let depth_view = depth.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.047,
                            g: 0.059,
                            b: 0.078,
                            a: 1.,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.surface);
            pass.set_bind_group(0, &group, &[]);
            pass.set_vertex_buffer(0, self.mesh.slice(..));
            pass.draw(0..self.vertices, 0..1);
        }
        let stride = (width * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPUI image transfer"),
            size: stride as u64 * height as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &output,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(height),
                },
            },
            extent,
        );
        let index = self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(index),
            timeout: Some(std::time::Duration::from_secs(5)),
        })?;
        rx.recv()??;
        let mapped = buffer.slice(..).get_mapped_range()?;
        let mut result = Vec::with_capacity((width * height * 4) as usize);
        for row in mapped.chunks(stride as usize) {
            result.extend_from_slice(&row[..(width * 4) as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(result)
    }
}
