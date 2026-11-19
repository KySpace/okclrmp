# Verification — 2026-11-19

Verified on Windows with Rust `1.101.0-nightly (249c1e4e3 2026-11-05)`, GPUI Kit 0.10.0, and NVIDIA GeForce RTX 4050 Laptop GPU. Native interaction checks used the computer-use skill against the built app. Clipboard and file-dialog checks ran the app with normal desktop access; restricted shell processes cannot access the same clipboard.

## Automated checks

| Check | Result |
| --- | --- |
| `cargo test --offline --all-targets` | 61 tests passed across the app and headless crate; 6 GPU/profile tests are opt-in; binary and example targets built successfully |
| `cargo test --offline -p okclrmp-core --doc` | Public loading/sampling and adaptive annotation examples pass |
| `cargo package --offline --allow-dirty -p okclrmp-core` | Standalone archive created and verified outside the app workspace |
| `cargo test --lib -- --ignored --nocapture` | All five hardware GPU tests and the Windows CMYK profile test passed |
| `cargo test -p accesskit_windows --lib unavailable_window_returns_uia_error_without_panicking` | 1 targeted teardown regression passed |
| `cargo clippy --all-targets -- -D warnings` | Passed |
| `cargo fmt --all --check` | Passed |
| `okclrmp-projection-preview.exe --check-gpu` | Passed on the actual RTX 4050; separate build preserves the running app |

GPU validation checks 368 C–L, 512 C–H, and 555 L–H interior pixels against the Rust OKLCH → sRGB conversion (1,435 comparisons, ≤2 byte-level difference). Filtered graticule pixels and pixels whose footprint touches the gamut boundary are excluded from this pointwise check. The earlier perspective preview contains 7,989 distinctly colored surface pixels with 4× MSAA. Image size and alpha are also checked. Four images are written under `.research/gpu-check/`.

Core tests cover conversion reference values and roundtrips, gamut boundaries, expression precedence/ternaries/constants/errors/builtins, cubic spline geometry and editing, legacy quadratic migration, spline channel ownership, slice intersections including jumps and tangencies, constant slice intervals, shortest-arc hue interpolation and 360° endpoints, saved project reproduction, import parsing, and output sample counts/endpoints/mapping policy.

## Headless colormap crate

`okclrmp-core` contains the six shared color/expression/constant/model/spline/exchange modules, unchanged apart from their location. The app retains compatibility re-exports and uses the same types and implementation. Dependency-tree inspection confirms only Serde and serde_json, with no UI, GPU, image, or Windows dependencies in the headless crate. Workspace default members include both packages, so the normal test command continues to run the extracted regressions.

Seven new API tests cover legacy project JSON, files/readers/direct Serde deserialization, expression constants and derived variables, immutability and Send/Sync use across threads, spline orthogonal constants and named slider synchronization in all three planes, imported paths crossing hue zero, output inversion, chroma mapping versus clipping, exact LUT reversal and independent LUT size, endpoint clamping, invalid documents and parameters, and singularities missed by the fixed load-time grid. A separate app integration regression checks all five actual sample projects under both inversion and mapping choices, comparing continuous colors and all 67 LUT entries against the app's exporter. Float comparisons use the same saved JSON on both sides to include the app's existing JSON parsing behavior.

Packaging verified `target/package/okclrmp-core-0.1.0.crate` in its extracted, standalone source tree. The included standard-library example loads `samples/linear.viridis.json` and writes a 512×256 synthetic heatmap to `.research/core-heatmap.ppm`. No app UI or rendering source changed; the hardware and native interaction results elsewhere in this document are from earlier checks.

The adaptive annotation function now lives in `okclrmp_core::helpers::annotation_ink`, with its body unchanged from the app's preview module. The app retains its original entry point through a re-export. A public doctest demonstrates converting a displayed sRGB background to OKLCH, choosing ink, and converting it back to RGBA8. The helper introduces no dependencies.

## Sample starting points and compact Curve studio

The five built-in definitions are literal Rust data; the app does not load sample projects at runtime. A regression independently compiles the sample JSON fixtures and compares all three coordinates at 1,025 parameter positions for each map, including Turbo's active cubic spline and its hue-zero crossing. It also checks that built-ins omit saved reference points, slider bindings, inactive splines, and view state. Another regression verifies that choosing a starting point preserves the existing camera, reference overlays, sample count, and RGB mapping choice, and that an edited Unicode description survives a project roundtrip without changing the curve.

The native picker groups maps by Linear, Symmetric, and Cyclic, with color strips on the button and menu entries. The description field in the top toolbar updates the native window title and the existing project `name` property. Parameter, variable, and expression guidance lives in label tooltips. The separate `target/debug/okclrmp-studio-preview.exe` is built for manual review of these controls; native interaction and tooltip placement have not been exercised by desktop automation for this change. Rendering code is unchanged, so the hardware results below are from previous checks.

## Cubic editing and cyclic hue

Geometry tests independently check the global first and second derivatives at a nonuniform middle parameter. They cover the two-step Corner action, nearest Smooth alignment, unchanged aligned handles, independent opposite-handle lengths, Auto C2 followed by a C1 edit, equal-length smoothing, anchor translation, two/three-node symmetry, shape-preserving subdivision, linear mode, Ctrl axis/direction snapping, and modifier release without cumulative drift. Old quadratic JSON projects migrate without changing their samples in all three planes, including unwrapped hue.

The cyclic view tests check shared hue panning modulo 360°, spline creation and anchor movement through hue zero, periodic handle hit targets, distinct selection of the 0°/360° endpoints, unchanged colormap exports after panning, and roundtripping the hue origin and polar flag. Polar geometry checks put every C=0 hue on the same vertical axis, keep chroma proportional to radius, and place white above black. The mesh regression moves the rectangular seam to five different hues, checks clipping and indices, and preserves each RGB boundary sample's lightness, chroma, and color.

`cargo test --lib cyclic_hue_and_polar_views_render_on_gpu -- --ignored --nocapture` passed. C–H and L–H each matched 40,960 quarter-turn pixel comparisons with no pixels beyond the 2-byte tolerance. C–L stays identical when the hue origin moves. Both volume modes produce identical images after a full 360° shift. The CPU-projected white-axis endpoint coincides with the white tip in the polar GPU image. All output pixels are opaque, including the odd-sized 675×401 volume captures. Images under `.research/cyclic-hue/` were visually inspected.

Four changed-origin renders at 675×240 measured 34.94 ms per rectangular render (including CPU seam clipping) and 1.94 ms per polar render on this adapter, including GPU work and readback. These debug-build measurements do not represent end-to-end native UI frame rates.

The new middle-button gestures and toolbar were compiled, and their geometry/controller paths were tested. Native automation of the separate spline preview was unavailable because Computer Use app approval timed out; no native gesture pass is claimed for these additions.

The main executable could not be replaced while the user's app was running (Windows error 5). A separate binary, `target/debug/okclrmp-hue-preview.exe`, built successfully from the current application sources and passed `--check-gpu`. The existing app session was left open.

The AccessKit test is intentionally targeted; the other upstream AccessKit tests were not run.

## Light and dark themes

The toolbar switches GPUI's global component theme and the app-owned palette together, then notifies the shared model so all docked panels redraw. The plot image cache includes appearance. Colormap conversion and project state are independent of theme selection.

`cargo test --lib themes_preserve_colormap_colors_and_refresh_plot_decoration_on_gpu -- --ignored --nocapture` passed on the RTX 4050. It checks 368 C–L, 512 C–H, and 555 L–H interior pixels for exact equality across themes, excluding graticule and gamut-boundary coverage. It also checks light/dark background values and opaque alpha in all three slices and both volume modes, and requires switching back to dark to restore every pixel exactly. Captures under `.research/themes/` were visually inspected. The existing hue and anti-aliasing GPU regressions also pass with the themed shader uniforms.

The separate `target/debug/okclrmp-theme-preview.exe` built successfully without replacing the running app. Native clicking of its theme toggle remains unverified because Computer Use app approval timed out. The light/dark image checks above are direct WGPU checks, not native UI screenshots.

## Compact plot layout

The default center layout now contains C–L/C–H/L–H across the top and output/gamut volume below. Reset removes existing side/bottom docks before reinstalling the default layout, so previously moved panels do not remain duplicated. Slice and parameter readouts reserve 88 logical pixels beside their sliders. Projection toolbars use SVG icons on one row, retain the hidden middle-mode group's width, and keep the edit toggle outside the scrolling tool region in narrow panels.

Operation help uses a cancellable two-second timer that restarts on pointer movement. The passive GPUI tooltip only renders while that position is still inside the plot; dragging, scrolling, clicking, pointer exit, and keyboard input dismiss it. No instructions take up plot-footer height.

The light-theme GPU test additionally requires the grid's contrast to exceed the checker pattern's contrast at independently selected out-of-gamut pixels. This passes with the lighter checkerboard while all 1,435 interior color comparisons remain identical across themes. The updated slice capture was visually inspected. Native layout and icon interaction checks remain unverified; a separate layout preview is provided for manual review.

Projection horizontal tick labels now sit at the top; the vertical C=0.40 and L=1.00 labels are omitted. Hue-zero labels sit one text row lower, with the dedicated marker clamped inside the right edge and coincident tick text suppressed. The output colorbar now uses the single-color annotation experiment described below in place of the earlier black/white halo.

## Spline orthogonal constant binding

The active spline plane has a constant toggle beside its edit button. Its slice value is the single source for the constant. Slider events rebuild the curve only when that slider owns the active constant; other slider changes remain view-only. The saved orthogonal expression stays intact and resumes when constant mode is disabled. Programmatic slider synchronization preserves saved f64 precision, and following the cursor preserves a constant H=360 endpoint.

Three model regressions cover all three owning planes, slider minimum/maximum/interior values, coplanarity, untouched spline channels, OKLCH export, unrelated sliders, restoring expressions, save/load, older JSON without the flag, switching expression/import/spline modes, inactive invalid expressions, and rejecting invalid slice values. The separate `target/debug/okclrmp-constant-preview.exe` provides a native build for manual visual review; native interaction checks remain unverified.

## Full zenith range, orthographic projection, and surface contours

Both coordinate modes support zenith 0–180° by clamping elevation to −π/2 through +π/2. The camera orbits its target and uses a yaw-dependent pitch tangent as its up vector at both poles. Orthographic zoom scales the frustum while keeping camera distance fixed; its scale matches perspective at the orbit center. The four controls are a single icon group with accessible names and tooltips. Reset preserves the coordinate, contour, and projection toggles.

Model and camera regressions check both orbit limits, switching coordinate modes without losing elevation, finite/invertible matrices and continuity at both poles across multiple yaw angles, projection of the orbit center, parallel rays at different depths, zoom scaling, clipping behind the camera, save/load, and defaults for older files missing the new flags.

`cargo test --lib full_zenith_and_orthographic_modes_render_on_gpu -- --ignored --nocapture` passed in twelve coordinate/projection/zenith combinations, each with contours off and on. Captures at `.research/projections/` cover rectangular/polar, perspective/orthographic, and above/side/below views; representative captures were visually inspected. The rectangular chart includes a neutral C=0 base, with a subdivided L³ gray ramp. A geometry regression verifies its complete area and neutral colors; the GPU regression confirms that the underside stays neutral when contours are disabled.

`cargo test --lib gamut_slice_contours_and_top_down_polar_render_on_gpu -- --ignored --nocapture` passed on the RTX 4050. The shader draws L/C/H level sets on the existing gamut boundary with derivative-based coverage and contrasting outlines. Rectangular mode uses three plane intersections; polar mode uses a lightness plane, chroma cylinder, and hue half-plane. Surface depth testing hides occluded contours. The image cache includes the contour flag and the slice values while enabled.

The hardware regression checks rectangular, oblique polar, and exactly top-down views in both themes. All three contour colors appear, moving the L slice changes the render, H=0/H=360 images match exactly, and a back-side hue slice does not draw an H+180 contour on the front. Sampled chart-cap interior pixels stay unchanged when a hue slice coincides with the seam; the cap area is excluded from the hue contour to avoid floating-point speckling. These view options leave the colormap samples unchanged. The twelve before/after renders and seam captures under `.research/surface-contours/` were visually inspected.

## Typed constants and reusable variables

Constant fields use precision limits of three digits for Fraction, one for Degree, two for Free, and zero for Integer, on initialization, dragging, Up/Down/wheel stepping, typing/pasting, and commit. The numeric regressions include rounding, precision reduction, floating point noise, negative zero formatting, retained scientific exponents, fractional thousandths, and hue tenths across 360°. Core project data and computed colormap samples retain their existing precision.

Six additional core tests cover the numeric operations used by the UI: drag rates, finite-value validation, degree wrapping (including floating point rounding at 360°), fraction limits, integer rounding, caret-relative steps on either side of the decimal, carry/borrow/sign changes, retained trailing zeroes, scientific mantissa/exponent stepping, and typed constraints during stepping. Expression tests check parameter-dependent definitions, references to constants and earlier definitions, ternaries, duplicate/reserved names, undefined/forward/cyclic references, and bounded evaluation complexity. Model tests independently compare derived L/C values against their formulas, tune a constant, save/load the typed rows and definitions, reject invalid values, and migrate legacy chained assignments without changing curve samples in all four presets.

The native row uses a horizontal resize cursor, a window-level drag listener while active, and a type menu. Double-click starts name editing; Enter moves focus to the numeric value and Escape restores the previous name. Numeric Up/Down handling captures keys before the text input's navigation actions. Wheel handling requires a focused, unselected numeric caret and accumulates fractional wheel deltas. Incomplete numeric text pauses export/save and retains the last valid curve; typed constraints normalize on Enter/blur. The separate definitions textarea shares the live rebuild path used by channel expressions.

`target/debug/okclrmp-variables-preview.exe` builds from the current app sources. Native automation returned `Computer Use was not approved to use okclrmp-variables-preview`; no native gesture or layout pass is claimed. Manual review should cover dragging a label beyond the editor bounds, Up/Down/wheel at different caret positions, all four type choices, add/rename/remove, live variable changes, and switching presets/projects and themes. The earlier GPU checks above are unchanged; GPU rendering code was not modified for this feature.

## Output inspection and fallback previews

Colorbar annotations convert the actual displayed, quantized sRGB background into OKLCH and apply `preview::annotation_ink`. The marker computes its ink once from the background beneath its line, and the parameter label reuses that exact color regardless of its placement. Endpoint labels sample their local backgrounds. Each is painted once with one color, without a halo, backplate, or second marker stroke. Sampling follows fallback conversion and output ordering. The earlier `.research/ink-preview/comparison.png` illustrates the previous position-based label rule; current native font rendering needs manual review.

The light startup theme and orthographic preset defaults compile with the new output panel. The perspective camera regression explicitly selects perspective before comparing it with orthographic projection; all five hardware regressions pass with the changed project defaults.

The colorbar owns the inspection parameter through click/drag, Left/Right, Shift, Home/End, and accessible slider actions. A window-level listener continues an active drag outside its bounds. The position helper is tested for interpolation, both clamped endpoints, zero width, and non-finite input. Inspection values and the slice-follow target icon sit below the bar beside a color/HEX copy button. A read-only textarea holds the full export string, and its content/scroll position are retained when only the inspection parameter changes.

The compact preview row groups fallback, clipping, preview sampling, and inversion; the output row groups sample count, encoding, copy, and export. Control rows stay on one line and allow horizontal scrolling in narrow docks. Preview sampling can be continuous, fixed-count, or linked to the export count. Two additional regressions verify swatch-center selection, clamped endpoints, continuous/discrete direction changes, independent preview counts, exact reversal of all four output encodings, unchanged curve evaluation, inversion save/load, and defaults for older files. Inversion updates preview direction, pointer/keyboard/accessibility movement, marker placement, and serialized order together.

The grayscale regression independently verifies preservation of linear-sRGB luminance. The opt-in CMYK regression ran against this machine's Windows sRGB and RSWOP profiles: it verifies actual saturation changes, finite bounded output, output count, black/white ordering, repeatability, and rejection of invalid inputs. Native transforms use 16-bit RGB → CMYK → RGB conversion, with profile/transform handles closed by RAII. Preview modes do not enter the colormap model or serialization path.

The separate `target/debug/okclrmp-compact-output-preview.exe` supports manual review of the compact rows, bar sampling/inversion, HEX copy, and complete serialized output. These new native interactions have not been exercised by desktop automation.

## Named constant slider bindings

Three model regressions cover all Degree/H and Fraction/L/C links, constant-to-slice and slice-to-constant updates, curves that reference the constants, direct chroma units and the 0.4 cap, occupied-slider rejection, incompatible type changes, unlinking, project roundtrips, defaults for older files, load-time slice synchronization, cyclic H=0/360, and view-only cursor following. For each spline plane they also verify shared orthogonal values, coplanarity, unchanged handles, and continued spline slider operation after the named link is removed. The numeric stepping regression checks that a C-bound fraction stops at 0.40.

Native sliders use the same model operations and synchronize the constant field without emitting another input change. Programmatic slider updates retain the existing f32 equality guard to preserve saved f64 values; user slider values clamp to the axis range before conversion reaches project validation. The link menu marks occupied axes, and slice readouts identify linked constants and the spline binding through their tooltip/accessibility label. Numeric readouts use the per-type precision limits above. A separate `target/debug/okclrmp-binding-preview.exe` supports manual review; the new menu and native gestures have been compiled but not exercised through desktop automation.

A separate `target/debug/okclrmp-projection-preview.exe` provides a native build for manual interaction review. No new native mouse-gesture pass is claimed.

## Anti-aliasing and mesh refinement

`cargo test --lib anti_aliasing_matches_area_reference_on_gpu -- --ignored --nocapture` passed on the RTX 4050. An independent 16×16 CPU area reference checks boundary coverage, rather than duplicating the shader's 4×4 sample pattern:

| Projection | Boundary pixels checked | Reduction in squared linear-color error vs point sampling |
| --- | --- | --- |
| C–L | 445 | 99.3% |
| C–H | 458 | 98.8% |
| L–H | 741 | 98.5% |

The GPU test also checks all four plots at the odd physical image size 257×161, including row padding, alpha, and multisampled depth/color resolve agreement. The DPI regression verifies 450×160 logical pixels become 675×240 physical pixels at 150% scale, and that the size limit preserves aspect ratio.

The geometry regression checks the actual saturated RGB creases against the old 80-step lightness grid, validates indices and hue-seam clipping, and keeps the pure-blue corner of the non-convex blue fold. The rectangular surface has 200,064 triangles, including the neutral base. Large GPU-rendered close-ups at 1536×640 were visually inspected from two camera angles: the repeated sawtooth ridge is absent, while real primary-color corners remain. Captures are under `.research/aa-after/gamut-closeup.png` and `gamut-orbit.png`.

Across 1,029 adaptive ridge columns, mean chroma loss relative to the true RGB crease falls from 0.0015516 for the old regular grid to 0.000000003 for the new mesh. A warm eight-render timing at 675×240 physical pixels, including readback, measured 0.59–0.67 ms for the slices and 1.44 ms for the new 3D mesh on this adapter. These are representative render timings, not an end-to-end UI frame-rate guarantee.

The existing app session remained open during this work. A separate native preview was built to avoid overwriting its locked executable. Computer Use permission for that preview was unavailable, so visual verification used the direct GPU captures instead of claiming a native-window screenshot check.

## Native interaction checks — initial prototype

| Scenario | Observed result |
| --- | --- |
| Select monochrome and diverging presets | Expressions, path, preview, and mode update |
| Set L to `sqrt(-1)` | Last valid curve remains; Save/Copy/Export disable; invalid status appears |
| Restore a valid preset | Error clears and export re-enables |
| Drag middle C–L spline node | All linked projections, gamut markings, 3D path, and output update |
| Inspect spline ownership | Other planes are locked while the selected plane owns two channels |
| Drag 3D viewport | Camera and gamut mesh orbit |
| Drag a dock tab and reset layout | Panels rearrange; Reset restores the initial layout |
| Import six HEX reference colors | Six overlay points appear with distance-dependent radius/opacity |
| Use reference colors as path | Piecewise OKLCH curve and output update; spline controls lock |
| Copy 256 HEX colors | Desktop clipboard JSON parsed and checked: 256 entries, `#000000` to `#ffffff` |
| Export through native Save As | `.research/ui-colormap.json` parsed and checked for the same count/endpoints |
| Save project through native Save As | `.research/ui-project.okclrmp.json` contains the parameter, three nodes, spline plane, and sample count |
| Change to diverging, then open saved project | Monochrome spline, expressions, preview, and mode restore |
| Click a projected path | Parameter slider and preview marker both move to `t ≈ 0.8193` |
| Drag lightness slice | C–H image and intersection label update to `L ≈ 0.796` |
| Close after accessibility inspection | Process exits 0 with empty stderr and no AccessKit panic |

## Fixes made during testing

- Output content now wraps and scrolls instead of squeezing the preview and clipping controls in a narrow dock.
- Clicking a path synchronizes the parameter slider with the model cursor.
- `sign(0)` and ties-to-even `round` follow the documented Julia-like scalar behavior.
- The output clipboard writer uses the live HWND, handles allocation ownership, and reports errors instead of claiming success after a failed write.
- A narrow vendored AccessKit patch returns `UIA_E_ELEMENTNOTAVAILABLE` for bounds requests racing window destruction, avoiding an abort across COM. See `vendor/accesskit_windows/LOCAL_PATCH.md`.
- GPU validation now exercises a grid of in-gamut pixels instead of a single point that could fall outside the gamut.

## Scope and limitations

This verifies the current Windows prototype. macOS/Linux, Display-P3/HDR, and zero-copy GPU sharing were not tested or implemented. WGPU renders cached offscreen images and copies them to GPUI's image API. Sampling is fixed-density, so extremely rapid functions or narrow unsampled singularities remain a limitation. Curve overlays in 3D remain visible through the gamut surface. See the README for the bridge compatibility decision and other prototype limitations.
