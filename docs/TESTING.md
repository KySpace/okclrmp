# Verification — 2026-11-17

Verified on Windows with Rust `1.101.0-nightly (249c1e4e3 2026-11-05)`, GPUI Kit 0.10.0, and NVIDIA GeForce RTX 4050 Laptop GPU. Native interaction checks used the computer-use skill against the built app. Clipboard and file-dialog checks ran the app with normal desktop access; restricted shell processes cannot access the same clipboard.

## Automated checks

| Check | Result |
| --- | --- |
| `cargo test --all-targets` | 14 core tests passed; binary test target built successfully |
| `cargo test -p accesskit_windows --lib unavailable_window_returns_uia_error_without_panicking` | 1 targeted teardown regression passed |
| `cargo clippy --all-targets -- -D warnings` | Passed |
| `cargo fmt --all --check` | Passed |
| `cargo run -- --check-gpu` | Passed on the actual RTX 4050 |

GPU validation checks 372 C–L, 519 C–H, and 563 L–H pixels against the Rust OKLCH → sRGB conversion (1,454 comparisons, ≤2 byte-level difference). Graticule pixels and numerically ambiguous gamut-boundary pixels are excluded. The 3D render contains 8,781 distinctly colored surface pixels. Image size and alpha are also checked. Four images are written under `.research/gpu-check/`.

Core tests cover conversion reference values and roundtrips, gamut boundaries, expression precedence/ternaries/constants/errors/builtins, quadratic interpolation, spline channel ownership, slice intersections including jumps and tangencies, constant slice intervals, shortest-arc hue interpolation and 360° endpoints, saved project reproduction, import parsing, and output sample counts/endpoints/mapping policy.

The AccessKit test is intentionally targeted; the other upstream AccessKit tests were not run.

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
