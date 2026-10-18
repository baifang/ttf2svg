# Sample icons

Five [Material Design icons](https://github.com/google/material-design-icons)
used to exercise `svg2mesh` on real-world SVG input:

| File | Icon |
| --- | --- |
| `computer.svg` | Desktop computer |
| `developer_board.svg` | Circuit board |
| `laptop.svg` | Laptop |
| `memory.svg` | Memory chip |
| `router.svg` | Router |

- Source: `https://github.com/google/material-design-icons/tree/master/src/hardware/<name>/materialicons`
- License: Apache License 2.0 — see `LICENSE` in this directory.

These are filled 24×24 icons, which makes them suitable for a fill-based
triangulator. Several of them contain a transparent `fill="none"` bounding
rectangle; `svg2mesh` skips unfilled paths, so only the visible glyph is
meshed.
