# ttf2svg

Two command-line tools for a font → SVG → mesh pipeline:

- **`ttf2svg`** reads a TrueType/OpenType font and writes one SVG per selected
  Unicode code point.
- **`svg2mesh`** reads a directory of SVGs and writes triangulated glyph meshes
  as C, Rust, and/or Wavefront OBJ — flat (2D) or extruded (3D).

```bash
ttf2svg font.ttf -r "0x0020-0x007E" -o out/svg
svg2mesh out/svg -o generated -n glyphs --lang all
```

Status: **0.1.x, pre-1.0**. The CLI may still change between minor releases.

## Install

```bash
cargo install --path .
```

Requires Rust 1.85 or newer. On Windows, the MSVC toolchain is recommended; if
your Visual Studio installation has no Windows SDK, see
[docs/windows.md](docs/windows.md).

To build without installing:

```bash
cargo build --release
# binaries in target/release/
```

## ttf2svg

```text
ttf2svg <FONT> [OPTIONS]
  -r, --range <SPEC>   ranges to export; repeatable; required
  -o, --out <DIR>      output directory; default ./out
  -q, --quiet          suppress warnings and summary
```

Range syntax: comma separated inclusive hexadecimal ranges, `0x` prefix,
case-insensitive, whitespace tolerated, single points allowed:

```text
0xAAAA-0xBBBB, 0xCCCC-0xCCCD
```

Overlaps are merged; reversed or surrogate ranges are rejected with exit code 2.
Output files are named `U+0041.svg`, contain a single `<path>`, and are
byte-for-byte deterministic.

### Exporting printable ASCII

```bash
ttf2svg font.ttf -r "0x0020-0x007E" -o ascii
```

U+0020 through U+007E is the full printable ASCII block: 95 code points, from
space to `~`. Space (U+0020) has no outlines, so it is counted as `empty` and
produces no file. Use `0x0021-0x007E` if you only want the 94 visible
characters.

For a font that covers the whole block, the result is:

```text
exported 94 glyphs (0 unmapped, 1 empty) to ascii
```

Code points the font does not map (some fonts omit punctuation) are reported as
`unmapped` and skipped rather than written.

## svg2mesh

```text
svg2mesh <INPUT_DIR> [OPTIONS]
  -o, --out <DIR>       output directory; default ./generated
  -n, --name <BASE>     output base name; default glyphs
  --lang <c|rust|obj|both|all>
                        outputs; default c (both = C+Rust, all = C+Rust+OBJ)
  --3d, -3              extruded 3D meshes instead of 2D
  -d, --depth <UNITS>   extrusion depth along Z; default 10.0
  -t, --tolerance <U>   curve flattening tolerance; default 0.5
  -q, --quiet
```

Note that `--lang` controls *which files* are written; `--3d` only changes the
mesh data.

### Supported SVG input

- Elements: `<path>`, `<rect>` (including `rx`/`ry` rounded corners),
  `<circle>`, `<ellipse>`, `<polygon>`, `<polyline>`, `<line>`.
- Path commands: `M L H V C S Q T A Z`, absolute and relative, with implicit
  repeats.
- Transforms on shapes and ancestors: `matrix`, `translate`, `scale`, `rotate`,
  `skewX`, `skewY`.
- Filled shapes only: paths whose nearest `fill` is `none` or `transparent` are
  skipped, so transparent bounding boxes do not become solid squares. A missing
  `fill` counts as filled (SVG's default is black).

Not interpreted: `<use>`, CSS rules, `fill-rule`, `clipPath`, `mask`, pattern
paints. Content under `defs`, `symbol`, `clipPath`, `mask`, `pattern` or
`marker` is ignored. Stroke-only artwork produces no mesh; convert strokes to
filled outlines first.

### Mesh quality

`-t, --tolerance <UNITS>` is the only quality control (default `0.5`). It is
the maximum deviation, in SVG units, allowed when curves are flattened into
line segments. Lower values mean smoother outlines and more triangles.

Measured on Segoe UI `O` (U+004F, 2 contours, 2D, 2048 units/em):

| `--tolerance` | vertices | triangles | `glyphs.c` |
| --- | --- | --- | --- |
| `2.0` | 128 | 128 | 6 KB |
| `0.5` (default) | 256 | 256 | 12 KB |
| `0.1` | 726 | 726 | 33 KB |
| `0.02` | 1903 | 1903 | 88 KB |

Suggested values:

| Use case | Tolerance |
| --- | --- |
| Small/embedded UI icons | `1.0` – `2.0` |
| Normal desktop use | `0.5` |
| Large display, print, 3D close-up | `0.1` – `0.25` |

Notes:

- The tolerance is **absolute**, in SVG/font units, not relative to glyph size
  or em: the same value is coarser for a small-upem font than for a large one.
- It applies to 2D and 3D alike, and to both caps and side walls.
- `--depth` changes geometry, not quality; `--3d` roughly doubles vertices.
- There is no mesh simplification stage: triangulation is exact for the
  flattened polygon, so tolerance is the only lever on triangle count.
- Indices are `uint16_t`, so one glyph is limited to 65535 vertices.

### C output

`--lang c` (or `both`/`all`) writes `glyphs.h` and `glyphs.c`. The header
exposes:

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

Add both files to your build; no other dependency, no allocation, no startup
work:

```bash
gcc -std=c99 -Wall -Wextra -c glyphs.c
gcc main.c glyphs.o -o app
```

```c
#include "glyphs.h"

const svg_mesh *glyph = svg_mesh_find(0x41);   /* 'A' */
if (glyph == NULL) {
    /* not exported */
}

const float *v = glyph->vertices;
for (uint32_t i = 0; i < glyph->index_count; i += 3) {
    uint16_t a = glyph->indices[i + 0];
    uint16_t b = glyph->indices[i + 1];
    uint16_t c = glyph->indices[i + 2];
    float ax = v[2 * a + 0], ay = v[2 * a + 1];
    /* draw triangle using a, b, c */
    (void)b; (void)c; (void)ax; (void)ay;
}
```

3D meshes use three floats per vertex, one normal per triangle at
`normals[3 * (i / 3)]`. Coordinates are in font units, origin at the glyph
origin on the baseline, y pointing **down** (SVG convention). `min_x`,
`min_y`, `max_x`, `max_y` are per-glyph bounds.

### Rust output

`--lang rust` writes a dependency-free `glyphs.rs` module:

```rust
mod glyphs;

fn main() {
    let glyph = glyphs::find('A' as u32).expect("glyph A");

    assert_eq!(glyphs::DIMENSIONS, 2); // 3 with --3d
    for triangle in glyph.indices.chunks_exact(3) {
        let a = triangle[0] as usize;
        let x = glyph.vertices[2 * a];
        let y = glyph.vertices[2 * a + 1];
        let _ = (x, y);
    }
}
```

The module provides `Mesh` (`name`, `code_point`, `bounds`, `vertices`,
`indices`, `normals`), `MESHES`, `COUNT`, `DIMENSIONS`, `find(code_point)`,
`find_by_name(name)`, and `Mesh::triangle_count()`. All data is `static`, the
module is `no_std`-friendly, and it compiles warning-free under `-D warnings`.

### OBJ output

`--lang obj` (or `all`) writes `glyphs.obj` with one `o <glyph>` object per
glyph:

- `v` lines in font units, `z = 0` for 2D.
- 2D faces: `f a b c`; 3D faces: `f a//n b//n c//n` with one `vn` per triangle.
- 1-based indices, global across the file.
- Y keeps the SVG down direction; flip it in your importer if needed.

In Blender, use *File → Import → Wavefront (.obj)*; scale by
`1 / units-per-em` afterwards if you need em-sized geometry.

### Example icons

[examples/icons](examples/icons) contains five Apache-2.0 Material Design
icons for trying the tool on real input.

## Development

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```

Fuzzing (nightly, via `cargo-fuzz`):

```bash
cargo +nightly fuzz run svg
cargo +nightly fuzz run font
```

See [AGENTS.md](AGENTS.md) for the project contract and module layout.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.

Third-party assets:

- `examples/icons/` — Material Design icons, Apache License 2.0.
- `tests/fixtures/Silkscreen-Regular.ttf` — Silkscreen, SIL Open Font
  License 1.1.
