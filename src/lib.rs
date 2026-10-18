#![forbid(unsafe_code)]

//! Internal library shared by the `ttf2svg` and `svg2mesh` binaries.
//!
//! This is not a stable public API: the crate exists to keep the two command
//! line tools and their tests on the same, tested parsing code.

pub mod codegen;
pub mod error;
pub mod font;
pub mod mesh;
pub mod obj;
pub mod range;
pub mod svg;
pub mod svgpath;
