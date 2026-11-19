//! Render a small synthetic heatmap without any UI, GPU, or image dependencies.
use okclrmp_core::Colormap;
use std::{
    fs::File,
    io::{BufWriter, Write},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let project = args
        .next()
        .ok_or("Usage: heatmap <project.json> [output.ppm]")?;
    let output = args.next().unwrap_or_else(|| "heatmap.ppm".into());
    let map = Colormap::load(project)?;
    let lut = map.rgba8_lut(256)?;
    let (width, height) = (512, 256);
    let mut file = BufWriter::new(File::create(output)?);
    write!(file, "P6\n{width} {height}\n255\n")?;
    for y in 0..height {
        for x in 0..width {
            let u = x as f64 / (width - 1) as f64;
            let v = y as f64 / (height - 1) as f64;
            let value =
                0.5 + 0.5 * (u * std::f64::consts::TAU).sin() * (v * std::f64::consts::PI).cos();
            let index = (value.clamp(0., 1.) * (lut.len() - 1) as f64).round() as usize;
            file.write_all(&lut[index][..3])?;
        }
    }
    file.flush()?;
    Ok(())
}
