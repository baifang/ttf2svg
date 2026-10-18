# AGENTS.md

Instructions for AI coding agents working in this repository.

Read this file before changing code. It records the project contract, the
commands that must pass, and the decisions that are easy to get wrong. If a
change makes something here untrue, update this file in the same change.

---

## 1. What this project is

A Rust package with two command line tools built on one internal library:

| Tool | Input | Output |
| --- | --- | --- |
| `ttf2svg` | `.ttf` / `.otf` font | one `U+XXXX.svg` per selected code point |
| `svg2mesh` | directory of `.svg` files | C, Rust, and/or OBJ triangle meshes |

Typical pipeline:

```bash
ttf2svg font.ttf -r "0x0041-0x005A, 0x0061-0x007A" -o out/svg
svg2mesh out/svg -o generated -n glyphs              # flat 2D meshes
svg2mesh out/svg -o generated -n glyphs --3d --depth 10
```

The range parameter is the reason `ttf2svg` exists: export a subset of a face
(an icon set, a CJK block, ASCII, …) instead of all ~60k glyphs.

Status: 0.1.x preview. CI covers Linux, macOS and Windows on stable and on the
MSRV.

---

## 2. Toolchain

- MSRV: Rust **1.85** (a dependency requires edition 2024). CI checks it.
- The default `rustup` toolchain is used; MSVC is recommended on Windows.
- If MSVC linking fails because no Windows SDK is installed, follow
  `docs/windows.md` (install the SDK, or build with MinGW-w64 using
  `.cargo/config.toml.example`).
- `.cargo/config.toml` is intentionally git-ignored: it is machine-specific and
  must not be committed.

Commands:

```bash
cargo build --release
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```

All three gates must be clean before a change is finished.

---

## 3. `ttf2svg` contract

```text
ttf2svg <FONT> [OPTIONS]

  -r, --range <SPEC>   code point ranges to export; repeatable; required
  -o, --out <DIR>      output directory; created if missing; default ./out
  -q, --quiet          suppress warnings and the summary line
```

- At least one `-r` is **required**. Never default to "whole font".
- Repeating `-r` accumulates: the union of all specs is exported.
- Exit codes: `0` success, `1` runtime failure, `2` usage error.
- stdout carries only the summary; warnings and errors go to stderr.
- Summary format (tests assert on it):
  `exported 94 glyphs (2 unmapped, 1 empty) to out`
- Printable ASCII is `-r "0x0020-0x007E"`; U+0020 is empty, so full coverage
  yields 94 files. `0x0021-0x007E` selects only the visible characters.
- An invalid range must not create or modify output files.

---

## 4. Range syntax

```text
spec        = range ( "," range )*
range       = point [ "-" point ]
point       = "0x" hexdigit+
hexdigit    = digit | "a".."f" | "A".."F"
```

| Rule | Behavior |
| --- | --- |
| Bounds | inclusive on both ends; `0x41-0x41` is one code point |
| Single point | `point` without `-` is a one-code-point range |
| Whitespace | ASCII whitespace around points, `-` and `,` is ignored |
| Case | `0x` and hex digits are case-insensitive |
| Leading zeros | allowed and irrelevant |
| Domain | `0x0 ..= 0x10FFFF`; above is an error |
| Surrogates | `0xD800 ..= 0xDFFF` rejected with a dedicated message |
| Reversed | `start > end` is an error, never swapped |
| Overlap | allowed; merged, deduplicated, exported ascending |
| Empty pieces | stray commas and `""` are usage errors |
| Other notations | `U+0041`, decimal and bare `41` are rejected |

Error messages name the offending text and the reason, then map to exit code 2:

```text
invalid range "0x5A-0x41": start 0x5A is greater than end 0x41
invalid range "0x110000": 0x110000 is outside Unicode (max 0x10FFFF)
invalid range "0xD800-0xDFFF": 0xD800..0xDFFF is a surrogate range
```

---

## 5. SVG output contract

- One SVG per exported code point, named `U+XXXX.svg` (uppercase hex, at least
  four digits; `U+1F600.svg` for astral planes).
- Minimal, deterministic document:

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -625 500 625" width="500" height="625">
  <path d="M125 0L125 -500Z" fill="currentColor"/>
</svg>
```

- One `<path>` per glyph; one `M` sub-path per contour, each closed with `Z`.
- Font y-up is flipped to SVG y-down: `x' = x`, `y' = -y`; `viewBox` is the
  flipped bounding box.
- Coordinates are rounded to integers in font units.
- Empty glyphs (space) write no file and count as `empty`.
- Unmapped code points are skipped, counted, and reported (up to five examples).
- Zero exported glyphs is an error (exit 1).

---

## 6. `svg2mesh` contract

```text
svg2mesh <INPUT_DIR> [OPTIONS]

  -o, --out <DIR>       output directory; default ./generated
  -n, --name <BASE>     base name of the generated files; default glyphs
  --lang <c|rust|obj|both|all>
                        outputs; default c (both = C+Rust, all = C+Rust+OBJ)
  --3d, -3              extruded 3D meshes instead of flat 2D
  -d, --depth <UNITS>   extrusion depth along Z; default 10.0
  -t, --tolerance <U>   curve flattening tolerance in SVG units; default 0.5
  -q, --quiet
```

- Reads top-level `*.svg` files only (no recursion), sorted by file name.
- Unreadable, unparsable, or empty files are warnings and are skipped. If no
  mesh was produced, exit `1`.
- A `U+XXXX` file stem becomes the code point; other names keep `code_point = 0`.
- Entries are sorted by code point, then name.
- Writes `<base>.h`/`<base>.c`, `<base>.rs`, and/or `<base>.obj` according to
  `--lang` into `--out`.

Supported SVG subset:

- Elements: `<path>`, `<rect>` (including rounded corners), `<circle>`,
  `<ellipse>`, `<polygon>`, `<polyline>`, `<line>`.
- Commands `M L H V C S Q T A Z`, absolute and relative, with implicit repeats.
- `transform` on shapes and ancestor elements: `matrix`, `translate`, `scale`,
  `rotate`, `skewX`, `skewY`; multiple shapes per document.
- Not interpreted: `<use>`, CSS rules, `fill-rule`, `clipPath`, `mask`, pattern
  paints. Anything under `defs`, `symbol`, `clipPath`, `mask`, `pattern` or
  `marker` is ignored.
- Paths whose nearest `fill` attribute is `none` or `transparent` are skipped;
  a missing `fill` counts as filled (SVG's default is black).

---

## 7. Mesh semantics

- Curves are flattened adaptively; the deviation is bounded by `--tolerance`.
- Contours with fewer than three points or ~zero area are dropped.
- Holes are detected by point-in-polygon containment and nesting parity.
- Even-depth contours are oriented counter-clockwise, odd-depth (hole) contours
  clockwise. This keeps the 3D side walls pointing outwards.
- Triangulation uses `earcutr` (ear clipping) per outer contour and its direct
  holes.
- 3D: front cap at `z = 0` (normal -Z), back cap at `z = depth` (normal +Z),
  side walls per contour edge; one normal per triangle.
- Indices are `uint16_t`: at most 65535 vertices. 3D needs two copies, so at
  most 65535 total. Exceeding the limit is an error, never a truncated mesh.

---

## 8. Generated code contract (C, Rust and OBJ)

### C

```c
#define SVG_MESH_COUNT 94u
#define SVG_MESH_DIMENSIONS 2u    /* or 3u */

typedef struct {
    const char *name;
    uint32_t code_point;
    float min_x, min_y, max_x, max_y;
    const float *vertices;        /* 2 or 3 floats per vertex */
    uint32_t vertex_count;
    const uint16_t *indices;      /* 3 per triangle */
    uint32_t index_count;
    const float *normals;         /* 3 per triangle; NULL in 2D */
} svg_mesh;

extern const svg_mesh svg_meshes[SVG_MESH_COUNT];
const svg_mesh *svg_mesh_find(uint32_t code_point);
```

- C99, `extern "C"` guarded, `#include <stdint.h>` and `<stddef.h>`.
- Per-glyph arrays are `static`; the table is the only exported data.
- Deterministic output: fixed 6-decimal floats, stable ordering, no timestamps.
- Generated code must compile with:
  `gcc -std=c99 -Wall -Wextra -Werror -c glyphs.c`

### Rust

`--lang rust` writes `<base>.rs`, a dependency-free module with the same data:

- `Mesh { name, code_point, bounds, vertices, indices, normals }`, with
  `normals: Option<&'static [f32]>` (always `None` in 2D).
- `MESHES: &'static [Mesh]`, `COUNT`, `DIMENSIONS`.
- `find(code_point)` and `find_by_name(name)` returning `Option<&'static Mesh>`.
- All arrays `static`, `#[allow(dead_code)]` on exported items so a partial
  consumer compiles warning-free with `-D warnings`.
- The module must remain valid as both `mod glyphs;` and `include!` content.

### OBJ

`--lang obj` writes `<base>.obj`; `--lang all` adds it to the C and Rust outputs.

- One `o` object per glyph; whitespace and `#` in names become `_`.
- `v` lines in font units, `z = 0` for 2D; 1-based global face indices.
- 3D: one `vn` per triangle, faces written as `a//n b//n c//n`.
- Coordinates keep the SVG y-down convention (documented for importers).
- Validation: every face vertex index must be within the vertex count and every
  normal index within the normal count.

---

## 9. Architecture

```text
src/lib.rs          internal library surface (not a public API)
src/range.rs        range-spec parser -> sorted disjoint ranges; std only
src/font.rs         face loading, cmap lookup, outline extraction
src/svg.rs          outline -> SVG text; no I/O, no font knowledge
src/svgpath.rs      SVG document/path parsing, transforms, arcs -> segments
src/mesh.rs         flatten, hole detection, triangulation, extrusion
src/codegen.rs      meshes -> C and Rust source strings; no I/O
src/obj.rs          meshes -> Wavefront OBJ text; no I/O
src/error.rs        typed errors for font loading
src/bin/ttf2svg.rs  CLI + filesystem pipeline for ttf2svg
src/bin/svg2mesh.rs CLI + filesystem pipeline for svg2mesh
```

Invariants:

- `range.rs` depends on `std` only and stays unit-testable without I/O.
- `svg.rs`, `svgpath.rs`, `mesh.rs` and `codegen.rs` never touch the filesystem.
- Only the two binaries decide process exit codes and write files.
- `#![forbid(unsafe_code)]` in the library and both binaries.

Dependencies: `clap`, `ttf-parser`, `earcutr`, `roxmltree`, `thiserror`, and
`anyhow` (binaries only). Justify any addition.

---

## 10. Testing

- Unit tests live beside the modules; CLI tests are in `tests/` and use
  `assert_cmd` + `predicates`.
- CLI tests create temporary directories under `target/test-tmp` (see the
  helpers) so they also work in sandboxes that restrict the system temp root.
- Fixture: `tests/fixtures/Silkscreen-Regular.ttf` (OFL 1.1). Keep fixtures
  small and licensed; record additions in `tests/fixtures/README.md`.
- `examples/icons/` holds five Apache-2.0 Material icons for manual input
  checks; generated output belongs under `out/`, never in `examples/`.
- Cover: range merging and errors, transform composition, shapes, holes, 2D/3D
  mesh counts, deterministic output, exit codes, end-to-end font → SVG → C/Rust,
  OBJ index validity, and the golden SVG in `tests/golden/`.
- Fuzz targets live in `fuzz/` (separate crate, excluded from packaging):
  `cargo +nightly fuzz run svg` and `... font`.
- Before finishing, compile a generated pair and an index-range check with gcc.

---

## 11. Conventions

- Rust 2021, MSRV pinned in `Cargo.toml` (`rust-version = "1.85"`).
- `cargo fmt` defaults; `clippy -D warnings` clean.
- No `unwrap()` / `expect()` on user input or font data. Tests may unwrap.
- Errors are lowercase, specific, and name the file or code point.
- Keep public items documented; the library is internal but still reviewed.

---

## 12. Non-goals

- Rasterization, PNG output, previews.
- Font subsetting or writing font binaries.
- Color tables, WOFF/WOFF2, `.ttc` collections, variable-font axes.
- Hinting, kerning, shaping, text layout.
- A public stable library API, a GUI, or a server.

---

## 13. Agent workflow

1. Read this file.
2. For nontrivial changes, state the plan and affected modules first.
3. Implement in small steps and add tests for new behavior.
4. Run every gate from §2.
5. Update `README.md` and this file when flags, output, or the range grammar
   change.
6. Keep diffs focused; do not reformat unrelated code.

### Definition of done

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test` passes with new cases
- [ ] `-r "0xAAAA-0xBBBB, 0xCCCC-0xCCCD"` works end to end
- [ ] 2D and `--3d` outputs both compile as C99 with `-Wall -Wextra -Werror`
- [ ] the generated Rust module compiles with `-D warnings`
- [ ] generated OBJ face and normal indices are all in range
- [ ] No panic paths reachable from malformed input
- [ ] Docs updated
