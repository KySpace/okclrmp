# Verification — 2026-11-17

Verified on Windows with Rust `1.101.0-nightly (249c1e4e3 2026-11-05)`, GPUI Kit 0.10.0, and NVIDIA GeForce RTX 4050 Laptop GPU. Native interaction checks used the computer-use skill against the built app. Clipboard and file-dialog checks ran the app with normal desktop access; restricted shell processes cannot access the same clipboard.

## Automated checks

| Check | Result |
| --- | --- |
| `cargo test --all-targets` | 16 core tests passed; GPU quality test is opt-in; binary test target built successfully |
| `cargo test -p accesskit_windows --lib unavailable_window_returns_uia_error_without_panicking` | 1 targeted teardown regression passed |
| `cargo clippy --all-targets -- -D warnings` | Passed |
| `cargo fmt --all --check` | Passed |
| `cargo run -- --check-gpu` | Passed on the actual RTX 4050 |

GPU validation checks 368 C–L, 512 C–H, and 555 L–H interior pixels against the Rust OKLCH → sRGB conversion (1,435 comparisons, ≤2 byte-level difference). Filtered graticule pixels and pixels whose footprint touches the gamut boundary are excluded from this pointwise check. The 3D render contains 8,532 distinctly colored surface pixels with 4× MSAA. Image size and alpha are also checked. Four images are written under `.research/gpu-check/`.

Core tests cover conversion reference values and roundtrips, gamut boundaries, expression precedence/ternaries/constants/errors/builtins, quadratic interpolation, spline channel ownership, slice intersections including jumps and tangencies, constant slice intervals, shortest-arc hue interpolation and 360° endpoints, saved project reproduction, import parsing, and output sample counts/endpoints/mapping policy.

The AccessKit test is intentionally targeted; the other upstream AccessKit tests were not run.

## Anti-aliasing and mesh refinement

`cargo test --lib anti_aliasing_matches_area_reference_on_gpu -- --ignored --nocapture` passed on the RTX 4050. An independent 16×16 CPU area reference checks boundary coverage, rather than duplicating the shader's 4×4 sample pattern:

| Projection | Boundary pixels checked | Reduction in squared linear-color error vs point sampling |
| --- | --- | --- |
| C–L | 445 | 99.3% |
| C–H | 458 | 98.8% |
| L–H | 741 | 98.5% |

The GPU test also checks all four plots at the odd physical image size 257×161, including row padding, alpha, and multisampled depth/color resolve agreement. The DPI regression verifies 450×160 logical pixels become 675×240 physical pixels at 150% scale, and that the size limit preserves aspect ratio.

The geometry regression checks the actual saturated RGB creases against the old 80-step lightness grid, validates indices and hue-seam clipping, and keeps the pure-blue corner of the non-convex blue fold. The new surface has 198,528 triangles. Large GPU-rendered close-ups at 1536×640 were visually inspected from two camera angles: the repeated sawtooth ridge is absent, while real primary-color corners remain. Captures are under `.research/aa-after/gamut-closeup.png` and `gamut-orbit.png`.

Across 1,029 adaptive ridge columns, mean chroma loss relative to the true RGB crease falls from 0.0015516 for the old regular grid to 0.000000003 for the new mesh. A warm eight-render timing at 675×240 physical pixels, including readback, measured 0.59–0.67 ms for the slices and 1.44 ms for the new 3D mesh on this adapter. These are representative render timings, not an end-to-end UI frame-rate guarantee.

The existing app session remained open during this work. A separate native preview was built to avoid overwriting its locked executable. Computer Use permission for that preview was unavailable, so visual verification used the direct GPU captures instead of claiming a native-window screenshot check.

## Native interaction checks

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
