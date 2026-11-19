# okclrmp-core

Load a project saved by OKLCH Colormap Studio and use its colormap in another Rust program. This crate uses only Serde and serde_json; it has no UI, GPU, image, or platform dependencies.

Use the local crate (it has not been published to crates.io):

```toml
[dependencies]
okclrmp-core = { path = "../okclrmp/crates/okclrmp-core" }
```

```rust,no_run
use okclrmp_core::Colormap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let map = Colormap::load("colormap.okclrmp.json")?;
    let color = map.sample(0.75)?;             // Original OKLCH coordinates
    let rgb = map.sample_srgb(0.75)?;          // Gamma-encoded sRGB, 0–1
    let rgba = map.sample_rgba8(0.75)?;        // [R, G, B, 255]
    let texture = map.rgba8_lut(256)?;         // Includes both endpoints
    Ok(())
}
```

`Colormap::from_json`, `from_reader`, and `from_project` are also available. `Colormap` implements Serde `Deserialize`, so `serde_json::from_str::<Colormap>(&json)` works directly. Expressions are compiled once on load. The immutable map is `Clone + Send + Sync` for parallel rendering.

Positions refer to the **output colorbar**, including its saved inversion. Finite positions outside `[0, 1]` clamp to the nearest endpoint; NaN/infinity return `SampleError`. Normalize heatmap values yourself with `(value - min) / (max - min)`, and handle missing data or a zero data range in your renderer. LUTs require at least two entries and are independent of the project's output sample count.

RGB sampling honors the project's saved chroma mapping or channel clipping choice. `sample` returns original OKLCH coordinates, including out-of-gamut values and unwrapped hue, without gamut mapping. To override the saved RGB policy, use `map.sample(t)?.rgba8(true)` for chroma mapping or `rgba8(false)` for clipping. RGB output is gamma encoded; use `okclrmp_core::color::decode` on its channels if your renderer expects linear-light RGB.

Existing project JSON is loaded through the same types, legacy quadratic/constant migration, slider-binding synchronization, and validation as the app. Expression, cubic/linear spline, orthogonal-constant, and imported-path modes are supported. The compiled map discards project descriptions and camera metadata. Advanced project/editing types remain available in `model`, `constants`, `expr`, `spline`, `color`, and `exchange`.

Expression validation checks the app's fixed set of 1,025 positions at load time. Expressions can still have singularities between them, so sampling also checks for non-finite results and returns an error rather than silently producing an invalid color. This crate evaluates expressions, not arbitrary host-language code.

For large heatmaps, build a LUT once and reuse it. The included example renders a synthetic heatmap as a binary PPM image using only the standard library:

```powershell
cargo run -p okclrmp-core --example heatmap -- samples/linear.viridis.json heatmap.ppm
```

No changes to the app or its saved project format are required.
