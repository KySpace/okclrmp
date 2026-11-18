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
- Use **Light theme** / **Dark theme** in the top toolbar to switch the current app session. Controls, docks, panels, plot backgrounds, axes, and editing overlays follow the theme. Colormap coordinates and exported colors stay unchanged.
- Edit the parameter name, constants, reusable variables, and channel expressions. Updates are live. Invalid input retains the last valid plot and disables export/save until corrected.
- Constants have individual name/value rows. Drag a name horizontally to tune its value; double-click the name to rename it. **+ Add** creates a constant, **×** removes it, and the type menu selects **Degree**, **Fraction**, **Integer**, or **Free**. With a caret in the numeric field and no text selected, Up/Down or the mouse wheel steps the digit immediately before the caret (skipping the decimal point). Written decimal precision and the selected place survive carries; scientific notation is supported. The wheel changes values only over the focused number field.
- Click the pencil toggle at a projection's top right to edit its spline. That plane owns its two channels; the omitted channel remains expression driven by default. Other projections show a lock with an explanatory tooltip. Click the same pencil again to return to the saved L/C/H expressions. The expression fields retain their previous formulas.
- While editing a spline, the constant toggle beside the pencil fixes the omitted channel at the current slice value: H for C–L, L for C–H, C for L–H. That plane's slider then changes both the slice and the colormap constant; its readout is highlighted. Other slice sliders remain view-only. Turn the toggle off to restore the saved expression. Constant mode and its value roundtrip with the project; leaving spline editing releases the binding.
- Drag numbered circular nodes or square Bézier handles. The curve icon toggles cubic/line mode; the middle-anchor icon toggles two/three nodes; the mirror icon toggles symmetric editing. Adding the middle node preserves the curve's shape, and removing it preserves endpoint tangents. Line mode uses two anchors without handles. The four middle-node mode icons reserve their toolbar space when hidden. Ctrl locks node movement to x/y or snaps a handle to x/y/its original direction.
- Middle-node tools: **Corner** first frees its tangents and then removes its handles on a second click. **Smooth** aligns tangents while retaining independent lengths. **Auto C2** solves first- and second-derivative continuity without moving anchors or endpoint handles; a subsequent manual edit retains C1. **Symm. smooth** maintains aligned, equal-length handles. **Symmetry** mirrors endpoint heights and handles about their moving vertical midpoint and keeps the middle anchor on that axis.
- Move each plot's slice slider; its value has a fixed-width readout beside the track. C–L fixes H; C–H fixes L; L–H fixes C. The whole curve is always projected. Colored dots label slice intersections. A curve entirely in a slice is identified below the plot; constant sub-intervals mark their endpoints. Clicking a projected path moves the inspection parameter.
- Middle-drag horizontally in **C–H**, **L–H**, or **Gamut volume** to pan their shared cyclic hue origin. Hue ticks wrap through 0° and a marker locates 0°. Panning moves the chart seam; spline nodes and handles can cross hue zero. Coordinates stay unwrapped in the model (for example, 350° → 370°). Panning changes neither curve values nor exports.
- The 3D view has H and L as its rectangular floor coordinates and C as height. Its colored surface follows the sRGB gamut boundary. Enable **Polar** for C as radius, H as azimuth, and L up the vertical C=0 axis, with white above black. Left-drag to orbit across zenith 0° (above), 90° (horizontal), and 180° (below), stopping at either pole. Both coordinate modes support the full range. Middle-drag to move the hue origin; scroll to zoom. Curve overlays remain visible through the surface, and out-of-gamut portions use coral dashes.
- The four gamut toolbar icons, from left to right, control **Polar**, **Slice contours**, **Orthographic projection**, and **Reset view**. Their tooltips describe the actions. Orthographic projection uses parallel rays and keeps the same scale at the orbit center when switching from perspective. Reset restores camera orientation, zoom, and hue origin while preserving these display toggles.
- Enable **Slice contours** in Gamut volume to show the current slice intersections on its boundary: blue for L, gold for C, purple for H. The projection sliders position these contours. In polar mode L slices are horizontal planes, C slices are cylinders, and H slices are radial half-planes. The contours follow the depth-tested surface, including its non-convex folds, and the toggle is saved with the project.
- Open **Reference colors** to paste or load a list, choose its input space, then **Plot colors**. Out-of-slice points are larger and fainter. **Use as colormap path** connects reference colors in their original order with piecewise linear OKLCH interpolation and shortest hue arcs.
- Choose the output sample count and format; copy colors or export a file. **Map chroma for RGB** reduces chroma at constant lightness/hue to the sRGB boundary. The alternative clips RGB channels. OKLCH CSV always exports the original path coordinates. A coral line beneath the preview marks out-of-gamut portions of the original path in either mode.
- The default layout has all three projections across the top, output at bottom left, gamut volume at bottom right, and Curve studio docked on the left. Drag dock tabs, resize splits, or use the panel menu to zoom. **Reset layout** restores this arrangement. **Save project** / **Open** roundtrip curve settings, handles, editing constraints, reference colors, slices, camera, hue origin, coordinate/projection modes, and slice-contour visibility as JSON. Older quadratic projects convert to cubic without changing their shape. Dock arrangement is session only.
- Rest the pointer over a plot for two seconds to see operation help. Movement restarts the delay; dragging, scrolling, clicking, or leaving the plot dismisses the tooltip. Individual toolbar icons have action-specific tooltips.

Smooth joins adjust the middle parameter in proportion to the incoming/outgoing handle lengths, so independent handle lengths maintain parametric C1 continuity. Auto C2 solves both derivatives at that parameter. Moving an anchor translates its handles with it, preserving their offsets; this may release Auto C2 to Smooth. With global symmetry enabled, middle smooth handles are horizontal and equal to satisfy both constraints.

## Output inspection and defaults

New sessions start in the light theme, and new preset projects use orthographic gamut projection. Saved projects retain their chosen projection mode.

In **Colormap output**, the top row groups the fallback dropdown, RGB clipping toggle, preview sampling dropdown, and inversion toggle. Preview sampling offers **Continuous**, fixed swatch counts, and **Use output count**, independently of export sampling. Click or drag the colorbar to inspect a value; sampled previews snap to their swatches. Left/Right steps one swatch, or 0.001 in continuous mode (0.01 with Shift); Home/End selects a displayed endpoint.

Below the bar, click the color/HEX button to copy the inspected color as HEX. The L/C/H values and target icon (**Move slices to this color**) share that row. Above the full, scrollable serialized text, the **Samples** and encoding dropdowns share one row with copy and export icons. Narrow docks scroll these control rows horizontally instead of wrapping them.

**Invert** reverses both the colorbar and exported sample order, and is saved with the project. The curve, its editable nodes, and the inspected source color stay unchanged; marker placement, scrubbing, keyboard movement, and endpoint labels follow the displayed direction. Older projects default to forward output.

The **Fallback** dropdown offers original sRGB, CMYK, and grayscale previews. CMYK uses Windows color transforms through the installed `RSWOP.icm` print profile, using relative colorimetric intent. This preview represents that SWOP printing condition; another press or paper needs its own profile. Grayscale preserves linear-sRGB luminance. Fallbacks affect the preview only; copied/exported samples keep their selected original format and mapping policy. CMYK is disabled with an explanation if the required system profiles are unavailable. See [Windows color transforms](https://learn.microsoft.com/en-us/windows/win32/api/icm/nf-icm-translatecolors).

## Expression language

One chosen parameter ranges over `[0, 1]`. Hue uses degrees. Functions without a `d` suffix use radians. Constants are named numeric rows, independent of the parameter. Degree values wrap into `[0, 360)`, fractions clamp to `[0, 1]`, integers round to whole numbers, and free values accept any finite number. Values normalize in the field on Enter or focus loss. Horizontal label dragging changes degrees by 0.5° per logical pixel, fractions by 0.002, integers by one per ten pixels, and free values by 0.01.

Constant value fields use type-specific decimal limits: **Fraction: 3**, **Degree: 1**, **Free: 2**, **Integer: 0**. Loading, dragging, stepping, and typed/pasted numbers follow these limits; trailing zeroes are optional. Curve computation and serialized output keep their existing precision.

Each constant's slider-link icon opens its binding menu: **Degree → H**, **Fraction → L or C**, or **Unbound**. One named constant can own each slider. Binding initially moves the slice to the constant's value; numeric edits and slider movement then update each other. C uses actual chroma in `[0, 0.4]`, so binding to C clamps a larger fraction to 0.4; L uses `[0, 1]`. Changing to an incompatible type removes the link. Bindings are saved in the project, and older projects start unbound.

Named links share the same slice value as the existing spline orthogonal-constant mode. Both can be active together; unlinking the named constant leaves the spline mode active. **Move slices to this color** preserves both kinds of bound axes to avoid feeding curve samples back into their inputs. At the hue slider's 360° endpoint, a degree constant reads 0° while the slice retains its equivalent 360° endpoint.

The separate **Variables** field accepts `name = expression` assignments separated by semicolons or newlines. Variables can use the parameter, constants, and earlier variables. Names must be distinct from the parameter, constants, builtins, and other variables. Forward references and cycles produce an error. Older project files with a combined constants string load as numeric rows, retaining their original values and curve samples.

```text
parameter: t
constant row: c_max = 0.30  [Fraction]

variables:
bias = 2 * abs(t - 0.5)
lightness = 1 - 0.36*bias

L: lightness
C: bias*c_max/2
H: t < 0.5 ? 15 : 263
```

Supported syntax: numbers including scientific notation, parentheses, unary `+ - !`, `+ - * / % ^`, `< <= > >= == !=`, `&& ||`, and `condition ? yes : no`. Exponentiation is right associative and binds more tightly than unary minus, as in Julia. Multiplication must be explicit. This is a scalar subset, not a Julia interpreter: no assignments in expressions, scripts, indexing, or implicit multiplication. Use `a < t && t < b` for combined comparisons.

Builtins: `pi`, `π`, `e`, `true`, `false`, `abs`, `sin`, `cos`, `tan`, `sind`, `cosd`, `sqrt`, `exp`, `log`/`ln`, `log10`, `floor`, `ceil`, `round`, `sign`, `deg2rad`, `rad2deg`, `min`, `max`, `atan2`, `mod`, `clamp`.

Expressions are checked on 1,025 parameter samples. Non-finite results produce an error; very narrow singularities between samples are not guaranteed to be detected. Projection lines break at the current chart seam and large sample jumps; the polar view connects continuously across hue zero. Intersections use sampled brackets, bisection with a residual check, and local minimization for tangencies. Extremely rapid oscillations may require denser/adaptive sampling in a future version. Large sample jumps are deliberately left disconnected instead of drawing misleading segments through discontinuities.

## Import / export formats

- **HEX** input: `#RGB` or `#RRGGBB`, separated by whitespace, commas, or JSON string array punctuation.
- **RGB 0–1** input: JSON `[[r,g,b], ...]` or CSV rows `r,g,b`, with each channel in `[0,1]`.
- **OKLCH** input: JSON triples or CSV rows `L,C,H`; H is degrees, L is normalized. Out-of-gamut coordinates are retained.
- Output: HEX JSON, normalized RGB JSON, RGB8 CSV (0–255), or OKLCH CSV. Endpoints at 0 and 1 are included.

CSV files contain values only, with no header. Reference lists are limited to 16,384 colors; output counts are 16, 64, 256, or 1,024 in the UI (the core accepts 2–4,096).

## Rendering and compatibility

The app uses **GPUI Kit 0.10.0**, including GPUI Component 0.10.0 and its pinned **gpui-pre 0.3.12**. WGPU 30 runs a compute shader for the three slice backgrounds and a depth-tested raster pipeline for the gamut surface. GPUI paints axes, curve segments, nodes, and reference overlays. Plot images use physical display pixels and are cached by viewport size, slice/camera values, hue origin, and volume mode; changing an expression does not regenerate an unchanged background. A uniform size cap preserves the aspect ratio on very large viewports. Projection ranges are L 0–1, C 0–0.4, and a movable cyclic 360° hue interval. L/C values outside the visible range remain in the model and export.

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

`color.rs` — OKLCH/OKLab/sRGB conversions and gamut search. `constants.rs` — typed numeric constraints, digit stepping, and legacy constant migration. `expr.rs` — closed scalar parser and reusable variables. `model.rs` — parametric curves, cyclic views, intersections, presets, project state. `spline.rs` — cubic Bézier geometry, constraints, snapping, and drag state. `exchange.rs` — import/export. `gpu.rs`, `gamut_mesh.rs`, and `shaders/` — WGPU images and boundary geometry. `ui.rs` and `ui/constant_editor.rs` — native Kit components, editing, docking, and interaction.

Inspired by [OKLCH Color Picker & Converter](https://oklch.com/). Color conversion matrices follow the [original OKLab specification](https://bottosson.github.io/posts/oklab/).
