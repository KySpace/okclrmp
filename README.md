# OKLCH Colormap Studio

A native Rust / GPUI workbench for designing colormaps as paths through OKLCH. Color coordinates, expressions, spline edits, reference points, and saved projects stay in OKLCH. RGB conversion happens at the display and import/export boundaries.

## Run

```powershell
cargo run
```

Requires Rust with the MSVC target, Visual Studio C++ Build Tools, a Windows SDK, CMake, and a working Direct3D 12 or Vulkan driver. The first build compiles GPUI Kit and WGPU and takes several minutes. The lockfile pins the resolved dependencies. Windows is the verified platform for this prototype.

```powershell
cargo test --lib
cargo run -- --check-gpu
```

The second command executes both WGSL pipelines on the actual adapter, compares slice pixels against the Rust color conversions, and writes four small images to `.research/gpu-check/`.

## Workflow

- Choose **Monochrome**, **Linear sweep**, **Diverging**, or **Turbo study**.
- Edit the parameter name, constants, and channel expressions. Updates are live. Invalid expressions retain the last valid plot and disable export until corrected.
- In expression mode, select **Edit spline** in one projection. That plane owns its two channels; the omitted channel remains expression driven. Other projections become read only. Return to **Expressions** to change planes. The expression fields retain their previous formulas.
- Drag numbered spline nodes. Two nodes define a line. Three define one interpolating quadratic passing through the middle node at parameter 0.5; the middle node is not a Bézier control point.
- Move each plot's slice slider. C–L fixes H; C–H fixes L; L–H fixes C. The whole curve is always projected. Colored dots label slice intersections. A curve entirely in a slice is identified below the plot; constant sub-intervals mark their endpoints. Clicking a projected path moves the inspection parameter.
- The 3D view has H and L as its rectangular floor coordinates and C as height. The colored roof is the maximum sRGB chroma at each H/L coordinate. Drag to orbit; scroll to zoom. Curve overlays remain visible through the surface, and out-of-gamut portions use coral dashes.
- Open **Reference colors** to paste or load a list, choose its input space, then **Plot colors**. Out-of-slice points are larger and fainter. **Use as colormap path** connects reference colors in their original order with piecewise linear OKLCH interpolation and shortest hue arcs.
- Choose the output sample count and format; copy colors or export a file. **Map chroma for RGB** reduces chroma at constant lightness/hue to the sRGB boundary. The alternative clips RGB channels. OKLCH CSV always exports the original path coordinates. A coral line beneath the preview marks out-of-gamut portions of the original path in either mode.
- Drag dock tabs, resize splits, or use the panel menu to zoom. **Reset layout** restores the initial arrangement. **Save project** / **Open** roundtrip curve settings, nodes, reference colors, slices, and camera state as JSON. Dock arrangement is session only.

## Expression language

One chosen parameter ranges over `[0, 1]`. Hue uses degrees. Functions without a `d` suffix use radians. Constants are scalar assignments separated by semicolons or newlines and may refer to earlier constants. They cannot refer to the parameter.

```text
parameter: t
constants: c_max=0.30; l_max=0.92; l_range=0.52

L: 1 - 0.36*2*abs(t - 0.5)
C: abs(t - 0.5)*c_max
H: t < 0.5 ? 15 : 263
```

Supported syntax: numbers including scientific notation, parentheses, unary `+ - !`, `+ - * / % ^`, `< <= > >= == !=`, `&& ||`, and `condition ? yes : no`. Exponentiation is right associative and binds more tightly than unary minus, as in Julia. Multiplication must be explicit. This is a scalar subset, not a Julia interpreter: no assignments in expressions, scripts, indexing, or implicit multiplication. Use `a < t && t < b` for combined comparisons.

Builtins: `pi`, `π`, `e`, `true`, `false`, `abs`, `sin`, `cos`, `tan`, `sind`, `cosd`, `sqrt`, `exp`, `log`/`ln`, `log10`, `floor`, `ceil`, `round`, `sign`, `deg2rad`, `rad2deg`, `min`, `max`, `atan2`, `mod`, `clamp`.

Expressions are checked on 1,025 parameter samples. Non-finite results produce an error; very narrow singularities between samples are not guaranteed to be detected. Projection lines break at hue seams and large sample jumps. Intersections use sampled brackets, bisection with a residual check, and local minimization for tangencies. Extremely rapid oscillations may require denser/adaptive sampling in a future version. Large sample jumps are deliberately left disconnected instead of drawing misleading segments through discontinuities.

## Import / export formats

- **HEX** input: `#RGB` or `#RRGGBB`, separated by whitespace, commas, or JSON string array punctuation.
- **RGB 0–1** input: JSON `[[r,g,b], ...]` or CSV rows `r,g,b`, with each channel in `[0,1]`.
- **OKLCH** input: JSON triples or CSV rows `L,C,H`; H is degrees, L is normalized. Out-of-gamut coordinates are retained.
- Output: HEX JSON, normalized RGB JSON, RGB8 CSV (0–255), or OKLCH CSV. Endpoints at 0 and 1 are included.

CSV files contain values only, with no header. Reference lists are limited to 16,384 colors; output counts are 16, 64, 256, or 1,024 in the UI (the core accepts 2–4,096).

## Rendering and compatibility

The app uses **GPUI Kit 0.10.0**, including GPUI Component 0.10.0 and its pinned **gpui-pre 0.3.12**. WGPU 30 runs a compute shader for the three slice backgrounds and a depth-tested raster pipeline for the gamut surface. GPUI paints axes, curve segments, nodes, and reference overlays. Plot images use physical display pixels and are cached by viewport size and slice/camera values; changing an expression does not regenerate an unchanged background. A uniform size cap preserves the aspect ratio on very large viewports. Plotting has a display range of L 0–1, C 0–0.4, and H 0–360; values outside it are preserved in the model and export but can leave the visible plot.

The slice shader integrates a 4×4 subpixel pattern at gamut boundaries and filters its graticule. The 3D pipeline uses 4× MSAA when the color/depth formats support it, resolves in linear light to an sRGB image, and filters grid lines with screen-space derivatives. The mesh follows the actual six RGB boundary faces transformed into OKLCH. Adaptive edge samples share the saturated creases, avoiding the sawtooth ridge produced by an H/L grid cutting across those creases. This also preserves the real non-convex fold near blue. Triangles crossing the hue seam are clipped and the exposed chart edges are closed to C=0.

Run the GPU quality regression explicitly:

```powershell
cargo test --lib anti_aliasing_matches_area_reference_on_gpu -- --ignored --nocapture
```

It compares gamut boundary pixels against an independent 16×16 area reference, checks odd image sizes, and writes small renders and large 3D close-ups under `.research/aa-after/`.

The requested zero-copy bridges were investigated:

- [bite-gpui/gpui_interop](https://github.com/bite-gpui/gpui_interop) 0.2.1 expects bite GPUI's `gpu_canvas`, `GpuCanvasContext`, and Windows shared-surface hooks. Kit's pinned upstream snapshot does not expose these APIs on Windows.
- [GPUI-3D](https://github.com/RandazzoMaxime/GPUI-3D) uses a vendored `gpui-ce` fork with its own shared-device surface API, not Kit's matching snapshot.

This prototype keeps current Kit and native WGPU using an **offscreen readback → BGRA RenderImage** bridge. It is not a zero-copy interop integration and does not use a webview or Three.js. GPU work and readback currently run synchronously when a cached plot image changes, so camera drags/resizes can stall on slow adapters. The renderer is isolated in `src/gpu.rs` to allow a shared-texture bridge or the requested webview fallback later. There is no claim that either upstream bridge is generally broken; the issue is compatibility with this exact Kit dependency graph.

The working gamut is **sRGB only**. Display-P3/HDR output, translucent slice planes in 3D, adaptive sampling, project undo/redo, numeric node editing, and persisted dock layouts are not implemented yet. The Turbo study is an editable spectral starting point, not the published Google Turbo lookup table.

On Windows the output's **Copy colors** action uses the live window as clipboard owner and reports write errors. This works around the pinned GPUI snapshot's null-owner clipboard write path; [Windows requires an owner for this sequence](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-openclipboard).

`vendor/accesskit_windows` contains a narrow teardown fix for AccessKit 0.34.0: accessibility bounds requests arriving after the HWND is destroyed return `UIA_E_ELEMENTNOTAVAILABLE` instead of aborting across COM. Its source licenses and patch notes are retained. Run its regression with `cargo test -p accesskit_windows --lib unavailable_window_returns_uia_error_without_panicking`.

## Source map

`color.rs` — OKLCH/OKLab/sRGB conversions and gamut search. `expr.rs` — closed scalar parser. `model.rs` — parametric curves, splines, intersections, presets, project state. `exchange.rs` — import/export. `gpu.rs` and `shaders/` — WGPU images. `ui.rs` — native Kit components, editing, docking, and interaction.

Inspired by [OKLCH Color Picker & Converter](https://oklch.com/). Color conversion matrices follow the [original OKLab specification](https://bottosson.github.io/posts/oklab/).
