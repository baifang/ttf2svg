# Fuzzing

The fuzz targets exercise the untrusted-input paths: SVG parsing and meshing,
and font parsing/outline extraction.

```bash
cargo install cargo-fuzz
cargo +nightly fuzz run svg
cargo +nightly fuzz run font
```

`cargo-fuzz` needs a nightly toolchain for sanitizer instrumentation. The
`fuzz/` directory is a separate crate, excluded from the published package.
