# Local AccessKit Windows patch

Upstream: `accesskit_windows` 0.34.0, from the Cargo registry, MIT OR Apache-2.0. Original source headers, manifest, README, and provenance are retained. Repository: https://github.com/AccessKit/accesskit

This patch makes a failed `ClientToScreen` return `UIA_E_ELEMENTNOTAVAILABLE` through the existing COM `Result` paths. Previously `unwrap()` aborted the process when an accessibility client queried bounds during native window destruction. Context, node, and text bounds queries propagate the error. Valid-window behavior is unchanged.

Regression check:

```powershell
cargo test -p accesskit_windows --lib unavailable_window_returns_uia_error_without_panicking
```

Remove the Cargo patch when an upstream release provides equivalent teardown handling.
