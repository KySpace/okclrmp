extern crate gpui_kit as gpui;
mod clipboard;
mod ui;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use okclrmp::gpu::Gpu;
use std::sync::Arc;

fn main() -> anyhow::Result<()> {
    let gpu = Arc::new(Gpu::new()?);
    if std::env::args().any(|a| a == "--check-gpu") {
        return check_gpu(&gpu);
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let display = cx
                .primary_display()
                .map(|d| d.bounds().size)
                .unwrap_or(size(px(1540.), px(1020.)));
            let bounds = Bounds::centered(
                None,
                size(
                    (display.width - px(32.)).min(px(1540.)),
                    (display.height - px(64.)).min(px(1020.)),
                ),
                cx,
            );
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("OKLCH Colormap Studio".into()),
                        ..Default::default()
                    }),
                    window_min_size: Some(size(px(1000.), px(640.))),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| ui::Studio::new(gpu, window, cx)),
            )
            .expect("Could not open OKLCH Studio");
            cx.activate(true);
        });
    Ok(())
}

fn check_gpu(gpu: &Gpu) -> anyhow::Result<()> {
    use okclrmp::{
        color::Oklch,
        model::{Plane, Project},
    };
    let p = Project::default();
    let w = 256u32;
    let h = 160u32;
    std::fs::create_dir_all(".research/gpu-check")?;
    for (i, plane) in [Some(Plane::Cl), Some(Plane::Ch), Some(Plane::Lh), None]
        .into_iter()
        .enumerate()
    {
        let pixels = gpu.render(plane, &p, w, h)?;
        anyhow::ensure!(pixels.len() == (w * h * 4) as usize, "Wrong buffer size");
        anyhow::ensure!(
            pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255),
            "Unexpected alpha"
        );
        if let Some(plane) = plane {
            let mut checked = 0;
            for y in (3..h).step_by(5) {
                for x in (3..w).step_by(7) {
                    let u = (x as f64 + 0.5) / w as f64;
                    let v = 1. - (y as f64 + 0.5) / h as f64;
                    // Exclude the deliberately blended graticule pixels.
                    let gx = (((u * 4. + 0.5).fract() - 0.5).abs()) * w as f64 / 4.;
                    let gy = ((((1. - v) * 4. + 0.5).fract() - 0.5).abs()) * h as f64 / 4.;
                    if gx.min(gy) < 1. {
                        continue;
                    }
                    let c = match plane {
                        Plane::Cl => Oklch::new(u, v * 0.4, p.slices[2]),
                        Plane::Ch => Oklch::new(p.slices[0], v * 0.4, u * 360.),
                        Plane::Lh => Oklch::new(v, p.slices[1], u * 360.),
                    };
                    let linear = c.linear_rgb();
                    // Avoid pixels that can disagree across f32 / f64 boundary tolerance.
                    let interior = [[-0.5, -0.5], [0.5, -0.5], [-0.5, 0.5], [0.5, 0.5]]
                        .iter()
                        .all(|[dx, dy]| {
                            let u = u + dx / w as f64;
                            let v = v + dy / h as f64;
                            let c = match plane {
                                Plane::Cl => Oklch::new(u, v * 0.4, p.slices[2]),
                                Plane::Ch => Oklch::new(p.slices[0], v * 0.4, u * 360.),
                                Plane::Lh => Oklch::new(v, p.slices[1], u * 360.),
                            };
                            c.linear_rgb().iter().all(|v| *v > 0.0001 && *v < 0.9999)
                        });
                    if interior && linear.iter().all(|v| *v > 0.0001 && *v < 0.9999) {
                        let expected = c.rgba8(false);
                        let offset = ((y * w + x) * 4) as usize;
                        for k in 0..3 {
                            anyhow::ensure!(
                                (pixels[offset + k] as i32 - expected[k] as i32).abs() <= 2,
                                "{plane:?} shader/CPU mismatch at ({x}, {y})"
                            );
                        }
                        checked += 1;
                    }
                }
            }
            anyhow::ensure!(
                checked > 20,
                "Not enough in-gamut pixels checked for {plane:?}"
            );
            println!("{plane:?}: {checked} GPU/CPU pixel comparisons passed");
        } else {
            let colored = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]) > 35)
                .count();
            anyhow::ensure!(
                colored > 500,
                "3D gamut surface is blank or missing its colors"
            );
            println!(
                "3D: {colored} colored surface pixels rendered · {}x MSAA",
                gpu.surface_sample_count()
            );
        }
        image::save_buffer(
            format!(".research/gpu-check/{i}.png"),
            &pixels,
            w,
            h,
            image::ColorType::Rgba8,
        )?;
    }
    println!(
        "WGPU check passed: {} · three WGSL slices and 3D gamut mesh",
        gpu.adapter_name
    );
    Ok(())
}
