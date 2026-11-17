//! WGPU renders offscreen; the Kit snapshot's image API is the presentation bridge.
//! Images are regenerated only for viewport / slice / camera changes, never per frame.
use crate::{
    appearance::Appearance,
    color::Oklch,
    gamut_mesh::{Mesh, Vertex},
    model::{Plane, Project},
};
use anyhow::{Context, Result};
use glam::{Mat4, Vec3};
use std::sync::Mutex;
use wgpu::util::DeviceExt;

const SURFACE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MAX_IMAGE_SIDE: u32 = 4096;

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    slices: wgpu::ComputePipeline,
    surface: wgpu::RenderPipeline,
    source: Mesh,
    polar_mesh: SurfaceBuffers,
    chart: Mutex<(f64, SurfaceBuffers)>,
    surface_samples: u32,
    pub adapter_name: String,
}
struct SurfaceBuffers {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}
impl SurfaceBuffers {
    fn new(device: &wgpu::Device, mesh: &Mesh) -> Self {
        Self {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Gamut vertices"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Gamut indices"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            count: mesh.indices.len() as u32,
        }
    }
}
pub fn world(c: Oklch, p: &Project) -> Vec3 {
    if p.polar {
        let angle = (c.h - p.hue_origin).to_radians();
        let radius = c.c / 0.4 * 1.2;
        Vec3::new(
            (radius * angle.cos()) as f32,
            (c.l * 1.8 - 0.9) as f32,
            (radius * angle.sin()) as f32,
        )
    } else {
        Vec3::new(
            (crate::model::hue_position(c.h, p.hue_origin) * 1.8 - 0.9) as f32,
            (c.c / 0.4 * 1.2 - 0.3) as f32,
            (c.l * 1.3 - 0.65) as f32,
        )
    }
}
pub fn camera(p: &Project, aspect: f32) -> Mat4 {
    let distance = 3.0 / p.zoom.clamp(0.5, 2.5);
    let eye = Vec3::new(
        distance * p.yaw.sin() * p.pitch.cos(),
        distance * p.pitch.sin(),
        distance * p.yaw.cos() * p.pitch.cos(),
    );
    Mat4::perspective_rh(40f32.to_radians(), aspect.max(0.1), 0.05, 50.)
        * Mat4::look_at_rh(
            eye,
            if p.polar {
                Vec3::ZERO
            } else {
                Vec3::new(0., 0.15, 0.)
            },
            Vec3::Y,
        )
}
pub fn project3(c: Oklch, p: &Project, aspect: f32) -> Option<[f64; 2]> {
    let v = camera(p, aspect) * world(c, p).extend(1.);
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
        // Color, resolve target, and depth must support the same sample count.
        let color_features = adapter.get_texture_format_features(SURFACE_FORMAT).flags;
        let depth_features = adapter.get_texture_format_features(DEPTH_FORMAT).flags;
        let surface_samples = if color_features.sample_count_supported(4)
            && color_features.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
            && depth_features.sample_count_supported(4)
        {
            4
        } else {
            1
        };
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
            label: Some("sRGB gamut boundary surface"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: surface_samples,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: SURFACE_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let source = crate::gamut_mesh::source();
        let polar_mesh = SurfaceBuffers::new(&device, &source);
        let chart = Mutex::new((0., SurfaceBuffers::new(&device, &source.chart(0.))));
        Ok(Self {
            device,
            queue,
            slices,
            surface,
            source,
            polar_mesh,
            chart,
            surface_samples,
            adapter_name,
        })
    }
    /// Physical render dimensions, capped uniformly to preserve the plot aspect.
    pub fn image_size(&self, logical_width: f32, logical_height: f32, scale: f32) -> [u32; 2] {
        image_size(logical_width, logical_height, scale, self.image_limit())
    }
    fn image_limit(&self) -> u32 {
        MAX_IMAGE_SIDE.min(self.device.limits().max_texture_dimension_2d)
    }
    pub fn surface_sample_count(&self) -> u32 {
        self.surface_samples
    }
    pub fn render(
        &self,
        plane: Option<Plane>,
        p: &Project,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>> {
        self.render_themed(plane, p, width, height, Appearance::Dark)
    }
    pub fn render_themed(
        &self,
        plane: Option<Plane>,
        p: &Project,
        width: u32,
        height: u32,
        appearance: Appearance,
    ) -> Result<Vec<u8>> {
        let style = appearance.plots();
        anyhow::ensure!(
            width > 0 && height > 0 && width <= self.image_limit() && height <= self.image_limit(),
            "Plot dimensions must be between 1 and {} physical pixels",
            self.image_limit()
        );
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
            format: if plane.is_some() {
                wgpu::TextureFormat::Rgba8Unorm
            } else {
                SURFACE_FORMAT
            },
            usage: wgpu::TextureUsages::COPY_SRC
                | if plane.is_some() {
                    wgpu::TextureUsages::STORAGE_BINDING
                } else {
                    wgpu::TextureUsages::RENDER_ATTACHMENT
                },
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
            let mut data = vec![
                width as f32,
                height as f32,
                plane,
                (p.hue_origin.rem_euclid(360.) / 360.) as f32,
                p.slices[0] as f32,
                p.slices[1] as f32,
                p.slices[2] as f32,
                0.,
            ];
            for color in [style.background, style.checker, style.grid] {
                data.extend(color);
                data.push(0.);
            }
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
            let mut matrix = camera(p, width as f32 / height as f32)
                .to_cols_array()
                .to_vec();
            matrix.extend([
                (p.hue_origin.rem_euclid(360.) / 360.) as f32,
                if p.polar { 1. } else { 0. },
                0.,
                0.,
            ]);
            matrix.extend(
                style
                    .volume_grid
                    .map(|v| crate::color::decode(v as f64) as f32),
            );
            matrix.push(0.);
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
                sample_count: self.surface_samples,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let depth_view = depth.create_view(&Default::default());
            let multisampled = (self.surface_samples > 1).then(|| {
                self.device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("4x MSAA gamut surface"),
                        size: extent,
                        mip_level_count: 1,
                        sample_count: self.surface_samples,
                        dimension: wgpu::TextureDimension::D2,
                        format: SURFACE_FORMAT,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            });
            let mut chart = self.chart.lock().unwrap();
            let origin = p.hue_origin.rem_euclid(360.);
            if !p.polar && chart.0 != origin {
                *chart = (
                    origin,
                    SurfaceBuffers::new(&self.device, &self.source.chart(origin)),
                );
            }
            let mesh = if p.polar { &self.polar_mesh } else { &chart.1 };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: multisampled.as_ref().unwrap_or(&view),
                    depth_slice: None,
                    resolve_target: multisampled.as_ref().map(|_| &view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: crate::color::decode(style.volume_background[0] as f64),
                            g: crate::color::decode(style.volume_background[1] as f64),
                            b: crate::color::decode(style.volume_background[2] as f64),
                            a: 1.,
                        }),
                        store: if multisampled.is_some() {
                            wgpu::StoreOp::Discard
                        } else {
                            wgpu::StoreOp::Store
                        },
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
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.count, 0, 0..1);
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

fn image_size(width: f32, height: f32, scale: f32, limit: u32) -> [u32; 2] {
    let scale = if scale.is_finite() && scale > 0. {
        scale
    } else {
        1.
    };
    let width = (width.max(1.) * scale).max(1.);
    let height = (height.max(1.) * scale).max(1.);
    let reduction = (limit as f32 / width.max(height)).min(1.);
    [
        (width * reduction).round().max(1.) as u32,
        (height * reduction).round().max(1.) as u32,
    ]
}

#[cfg(test)]
mod tests {
    use super::{Gpu, image_size, project3, world};
    use crate::{
        color::{Oklch, decode},
        model::{Plane, Project},
    };

    #[test]
    fn physical_images_follow_dpi_and_preserve_aspect_at_the_limit() {
        assert_eq!(image_size(450., 160., 1.5, 4096), [675, 240]);
        assert_eq!(image_size(450., 160., 2., 4096), [900, 320]);
        assert_eq!(image_size(3000., 1500., 2., 4096), [4096, 2048]);
        assert_eq!(image_size(1., 0., 1., 4096), [1, 1]);
    }
    #[test]
    fn polar_axis_is_achromatic_lightness_up_with_periodic_hue_azimuth() {
        let mut p = Project {
            polar: true,
            ..Default::default()
        };
        for hue in [0., 90., 215., 360., 720.] {
            let black = world(Oklch::new(0., 0., hue), &p);
            let white = world(Oklch::new(1., 0., hue), &p);
            assert_eq!(black, glam::Vec3::new(0., -0.9, 0.));
            assert_eq!(white, glam::Vec3::new(0., 0.9, 0.));
            let point = world(Oklch::new(0.6, 0.2, hue), &p);
            let wrapped = world(Oklch::new(0.6, 0.2, hue + 360.), &p);
            assert!((point - wrapped).length() < 1e-6);
            assert!((point.x.hypot(point.z) - 0.6).abs() < 1e-6);
        }
        let a = world(Oklch::new(0.5, 0.2, 90.), &p);
        p.hue_origin = 90.;
        let b = world(Oklch::new(0.5, 0.2, 90.), &p);
        assert!(a.x.abs() < 1e-6 && b.z.abs() < 1e-6);
        assert!((a.z - b.x).abs() < 1e-6);
        let black = project3(Oklch::new(0., 0., 0.), &p, 1.5).unwrap();
        let white = project3(Oklch::new(1., 0., 0.), &p, 1.5).unwrap();
        assert!(white[1] > black[1]);
    }

    #[test]
    #[ignore = "requires a real WGPU adapter; writes .research/cyclic-hue images"]
    fn cyclic_hue_and_polar_views_render_on_gpu() {
        let gpu = Gpu::new().unwrap();
        let mut p = Project::default();
        let (w, h) = (256u32, 160u32);
        std::fs::create_dir_all(".research/cyclic-hue").unwrap();
        for plane in Plane::ALL {
            p.hue_origin = 0.;
            let baseline = gpu.render(Some(plane), &p, w, h).unwrap();
            p.hue_origin = 90.;
            let shifted = gpu.render(Some(plane), &p, w, h).unwrap();
            if plane == Plane::Cl {
                assert_eq!(baseline, shifted);
            } else {
                let mut bad = 0;
                for y in 0..h {
                    for x in 0..w {
                        let a = ((y * w + (x + w / 4) % w) * 4) as usize;
                        let b = ((y * w + x) * 4) as usize;
                        if (0..3)
                            .any(|k| (baseline[a + k] as i32 - shifted[b + k] as i32).abs() > 2)
                        {
                            bad += 1;
                        }
                    }
                }
                assert!(
                    bad < 20,
                    "{plane:?}: {bad} pixels fail cyclic quarter-turn comparison"
                );
                println!(
                    "{plane:?}: {} cyclic shift pixel comparisons, {bad} beyond tolerance",
                    w * h
                );
            }
            p.hue_origin = 450.;
            assert_eq!(shifted, gpu.render(Some(plane), &p, w, h).unwrap());
            image::save_buffer(
                format!(".research/cyclic-hue/{plane:?}.png"),
                &shifted,
                w,
                h,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        for polar in [false, true] {
            p.polar = polar;
            p.hue_origin = 57.;
            let shifted = gpu.render(None, &p, 675, 401).unwrap();
            assert!(shifted.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
            let colored = shifted
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]) > 35)
                .count();
            assert!(colored > 10000, "Empty gamut surface: polar={polar}");
            p.hue_origin = 417.;
            assert_eq!(shifted, gpu.render(None, &p, 675, 401).unwrap());
            image::save_buffer(
                format!(
                    ".research/cyclic-hue/{}.png",
                    if polar { "polar" } else { "rectangular" }
                ),
                &shifted,
                675,
                401,
                image::ColorType::Rgba8,
            )
            .unwrap();
            println!("polar={polar}: {colored} colored pixels; periodic rotation matches exactly");
            if polar {
                let white = project3(Oklch::new(1., 0., 0.), &p, 675. / 401.).unwrap();
                let (x, y) = ((white[0] * 675.) as i32, ((1. - white[1]) * 401.) as i32);
                assert!(
                    (-4..=4).any(|dy| (-4..=4).any(|dx| {
                        let (x, y) = (x + dx, y + dy);
                        if !(0..675).contains(&x) || !(0..401).contains(&y) {
                            return false;
                        }
                        let i = ((y * 675 + x) * 4) as usize;
                        shifted[i..i + 3].iter().all(|c| *c > 220)
                    })),
                    "CPU white-axis endpoint must coincide with the GPU white tip"
                );
            }
            let start = std::time::Instant::now();
            for origin in [15., 35., 75., 125.] {
                p.hue_origin = origin;
                gpu.render(None, &p, 675, 240).unwrap();
            }
            println!(
                "polar={polar}: {:.2} ms/changed-origin render (mesh + GPU + readback)",
                start.elapsed().as_secs_f64() * 1000. / 4.
            );
        }
    }

    fn slice_color(plane: Plane, p: &Project, x: f64, y: f64, w: u32, h: u32) -> Oklch {
        let u = x / w as f64;
        let v = 1. - y / h as f64;
        match plane {
            Plane::Cl => Oklch::new(u, v * 0.4, p.slices[2]),
            Plane::Ch => Oklch::new(p.slices[0], v * 0.4, u * 360.),
            Plane::Lh => Oklch::new(v, p.slices[1], u * 360.),
        }
    }
    fn valid(rgb: [f64; 3]) -> bool {
        rgb.iter().all(|v| *v >= -0.00001 && *v <= 1.00001)
    }

    #[test]
    #[ignore = "requires a real WGPU adapter; writes .research/themes images"]
    fn themes_preserve_colormap_colors_and_refresh_plot_decoration_on_gpu() {
        use crate::appearance::Appearance;
        let gpu = Gpu::new().unwrap();
        let mut p = Project::default();
        let (w, h) = (256u32, 160u32);
        std::fs::create_dir_all(".research/themes").unwrap();
        for plane in [
            Some(Plane::Cl),
            Some(Plane::Ch),
            Some(Plane::Lh),
            None,
            None,
        ] {
            let dark = gpu
                .render_themed(plane, &p, w, h, Appearance::Dark)
                .unwrap();
            let light = gpu
                .render_themed(plane, &p, w, h, Appearance::Light)
                .unwrap();
            assert_eq!(dark.len(), light.len());
            assert!(light.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
            let corner = ((3 * w + 3) * 4) as usize;
            assert!(dark[corner..corner + 3].iter().all(|c| *c < 45));
            assert!(light[corner..corner + 3].iter().all(|c| *c > 220));
            if plane == Some(Plane::Cl) {
                // These three pixels lie outside the gamut. The grid should
                // stand out more than the checker pattern beneath it.
                let pixel = |x: u32| {
                    let i = ((21 * w + x) * 4) as usize;
                    &light[i..i + 3]
                };
                let checker_contrast: i32 = pixel(55)
                    .iter()
                    .zip(pixel(61))
                    .map(|(a, b)| *a as i32 - *b as i32)
                    .sum();
                let grid_contrast: i32 = pixel(61)
                    .iter()
                    .zip(pixel(63))
                    .map(|(a, b)| *a as i32 - *b as i32)
                    .sum();
                assert!(
                    grid_contrast > checker_contrast,
                    "The light-mode grid must contrast more than the checker pattern: {grid_contrast} vs {checker_contrast}"
                );
            }
            if let Some(plane) = plane {
                let mut checked = 0;
                for y in (3..h - 1).step_by(5) {
                    for x in (3..w - 1).step_by(7) {
                        let gx = ((((x as f64 + 0.5) / w as f64 * 4. + 0.5).fract() - 0.5).abs())
                            * w as f64
                            / 4.;
                        let gy = ((((y as f64 + 0.5) / h as f64 * 4. + 0.5).fract() - 0.5).abs())
                            * h as f64
                            / 4.;
                        if gx.min(gy) < 1. {
                            continue;
                        }
                        let interior =
                            [[0., 0.], [1., 0.], [0., 1.], [1., 1.]]
                                .iter()
                                .all(|[dx, dy]| {
                                    slice_color(plane, &p, x as f64 + dx, y as f64 + dy, w, h)
                                        .linear_rgb()
                                        .iter()
                                        .all(|c| *c > 0.0001 && *c < 0.9999)
                                });
                        if interior {
                            let i = ((y * w + x) * 4) as usize;
                            assert_eq!(
                                dark[i..i + 4],
                                light[i..i + 4],
                                "Theme altered {plane:?} colormap color at ({x},{y})"
                            );
                            checked += 1;
                        }
                    }
                }
                assert!(checked > 20);
                println!("{plane:?}: {checked} colormap interior pixels identical across themes");
            }
            assert_eq!(
                dark,
                gpu.render_themed(plane, &p, w, h, Appearance::Dark)
                    .unwrap(),
                "Switching back must restore the dark plot exactly"
            );
            let name = plane
                .map(|p| format!("{p:?}"))
                .unwrap_or_else(|| if p.polar { "polar" } else { "volume" }.into());
            for (theme, pixels) in [("dark", dark), ("light", light)] {
                image::save_buffer(
                    format!(".research/themes/{name}-{theme}.png"),
                    &pixels,
                    w,
                    h,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            if plane.is_none() {
                p.polar = true;
            }
        }
    }

    /// An independent 16x16 area reference checks the silhouette rather than
    /// duplicating the shader's lower-density sampling pattern.
    #[test]
    #[ignore = "requires a real WGPU adapter; writes .research/aa-after images"]
    fn anti_aliasing_matches_area_reference_on_gpu() {
        let gpu = Gpu::new().unwrap();
        let p = Project::default();
        let (w, h) = (256u32, 160u32);
        std::fs::create_dir_all(".research/aa-after").unwrap();
        for (index, plane) in Plane::ALL.into_iter().enumerate() {
            let pixels = gpu.render(Some(plane), &p, w, h).unwrap();
            let (mut count, mut aa_error, mut point_error) = (0, 0., 0.);
            for y in 1..h - 1 {
                for x in 1..w - 1 {
                    let gx = ((((x as f64 + 0.5) / w as f64 * 4. + 0.5).fract() - 0.5).abs())
                        * w as f64
                        / 4.;
                    let gy = ((((y as f64 + 0.5) / h as f64 * 4. + 0.5).fract() - 0.5).abs())
                        * h as f64
                        / 4.;
                    if gx.min(gy) < 1. {
                        continue;
                    }
                    let corners = [[0., 0.], [1., 0.], [0., 1.], [1., 1.]].map(|[dx, dy]| {
                        valid(
                            slice_color(plane, &p, x as f64 + dx, y as f64 + dy, w, h).linear_rgb(),
                        )
                    });
                    if corners.iter().all(|v| *v) || corners.iter().all(|v| !*v) {
                        continue;
                    }
                    let background = if (x / 8 + y / 8) % 2 == 0 {
                        [0.085, 0.098, 0.12]
                    } else {
                        [0.102, 0.116, 0.14]
                    }
                    .map(decode);
                    let sample = |sx: f64, sy: f64| {
                        let rgb = slice_color(plane, &p, sx, sy, w, h).linear_rgb();
                        if valid(rgb) {
                            rgb.map(|v| v.clamp(0., 1.))
                        } else {
                            background
                        }
                    };
                    let mut reference = [0.; 3];
                    for sy in 0..16 {
                        for sx in 0..16 {
                            let value = sample(
                                x as f64 + (sx as f64 + 0.5) / 16.,
                                y as f64 + (sy as f64 + 0.5) / 16.,
                            );
                            for k in 0..3 {
                                reference[k] += value[k] / 256.;
                            }
                        }
                    }
                    let point = sample(x as f64 + 0.5, y as f64 + 0.5);
                    let offset = ((y * w + x) * 4) as usize;
                    for k in 0..3 {
                        aa_error +=
                            (decode(pixels[offset + k] as f64 / 255.) - reference[k]).powi(2);
                        point_error += (point[k] - reference[k]).powi(2);
                    }
                    count += 1;
                }
            }
            assert!(count > 20, "Too few boundary pixels checked for {plane:?}");
            assert!(
                aa_error < point_error * 0.6,
                "{plane:?}: AA error {aa_error} vs point-sampled {point_error}"
            );
            println!(
                "{plane:?}: {count} boundary pixels, {:.1}% less area-reference error",
                100. * (1. - aa_error / point_error)
            );
            image::save_buffer(
                format!(".research/aa-after/{index}.png"),
                &pixels,
                w,
                h,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        let pixels = gpu.render(None, &p, w, h).unwrap();
        image::save_buffer(
            ".research/aa-after/3.png",
            &pixels,
            w,
            h,
            image::ColorType::Rgba8,
        )
        .unwrap();
        assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        println!(
            "3D: {}x MSAA, {} triangles on {}",
            gpu.surface_sample_count(),
            gpu.chart.lock().unwrap().1.count / 3,
            gpu.adapter_name
        );
        let closeup = Project {
            zoom: 1.65,
            pitch: 0.55,
            ..p.clone()
        };
        let pixels = gpu.render(None, &closeup, 1536, 640).unwrap();
        image::save_buffer(
            ".research/aa-after/gamut-closeup.png",
            &pixels,
            1536,
            640,
            image::ColorType::Rgba8,
        )
        .unwrap();
        let orbit = Project {
            yaw: 0.45,
            pitch: 0.25,
            zoom: 1.2,
            ..p.clone()
        };
        let pixels = gpu.render(None, &orbit, 1536, 640).unwrap();
        image::save_buffer(
            ".research/aa-after/gamut-orbit.png",
            &pixels,
            1536,
            640,
            image::ColorType::Rgba8,
        )
        .unwrap();
        // Odd physical sizes exercise row padding, compute bounds checks, and
        // multisampled color/depth/resolve size agreement after DPI scaling.
        for plane in [Some(Plane::Cl), Some(Plane::Ch), Some(Plane::Lh), None] {
            let pixels = gpu.render(plane, &p, 257, 161).unwrap();
            assert_eq!(pixels.len(), 257 * 161 * 4);
            assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        }
    }
}
