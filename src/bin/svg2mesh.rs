#![forbid(unsafe_code)]

//! `svg2mesh`: turn a directory of SVG glyphs into C and/or Rust mesh tables.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, ValueEnum};

use ttf2svg::codegen::{generate, generate_rust, GlyphMesh};
use ttf2svg::mesh::{build_2d, build_3d, Mesh};
use ttf2svg::obj;
use ttf2svg::svgpath::parse_document;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Lang {
    C,
    Rust,
    Obj,
    /// C and Rust
    Both,
    /// C, Rust and OBJ
    All,
}

#[derive(Debug, Parser)]
#[command(
    name = "svg2mesh",
    version,
    about = "Convert a directory of SVG glyphs into C, Rust, or OBJ mesh data",
    after_help = "Examples:\n  svg2mesh out/ -o generated -n glyphs\n  svg2mesh out/ -o generated -n glyphs --3d --depth 20\n  svg2mesh out/ -o generated -n glyphs --lang rust\n  svg2mesh out/ -o generated -n glyphs --lang obj"
)]
struct Cli {
    /// Directory containing .svg files (usually the output of ttf2svg)
    #[arg(value_name = "INPUT_DIR")]
    input: PathBuf,

    /// Output directory; created if missing
    #[arg(
        short = 'o',
        long = "out",
        value_name = "DIR",
        default_value = "generated"
    )]
    out: PathBuf,

    /// Base name of the generated files
    #[arg(
        short = 'n',
        long = "name",
        value_name = "BASE",
        default_value = "glyphs"
    )]
    name: String,

    /// Outputs: c, rust, obj, both (C+Rust), all (C+Rust+OBJ)
    #[arg(long = "lang", value_name = "LANG", value_enum, default_value = "c")]
    lang: Lang,

    /// Emit extruded 3D meshes instead of flat 2D meshes
    #[arg(long = "3d", short = '3')]
    three_d: bool,

    /// Extrusion depth along Z, in SVG units (3D only)
    #[arg(
        short = 'd',
        long = "depth",
        value_name = "DEPTH",
        default_value_t = 10.0
    )]
    depth: f64,

    /// Curve flattening tolerance in SVG units
    #[arg(
        short = 't',
        long = "tolerance",
        value_name = "UNITS",
        default_value_t = 0.5
    )]
    tolerance: f64,

    /// Suppress warnings and the summary line
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if !(cli.tolerance.is_finite() && cli.tolerance > 0.0) {
        eprintln!("error: tolerance must be a positive number");
        return ExitCode::from(2);
    }

    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> Result<()> {
    let mut paths = svg_files(&cli.input)?;
    if paths.is_empty() {
        return Err(anyhow!("no .svg files found in {}", cli.input.display()));
    }
    paths.sort();

    let mut entries = Vec::new();
    let mut skipped = 0_usize;

    for path in paths {
        match load_mesh(&path, cli) {
            Ok(entry) => entries.push(entry),
            Err(error) => {
                skipped += 1;
                if !cli.quiet {
                    eprintln!("warning: skipping {}: {error:#}", path.display());
                }
            }
        }
    }

    entries.sort_by(|a, b| {
        a.code_point
            .cmp(&b.code_point)
            .then_with(|| a.name.cmp(&b.name))
    });

    let Some(_) = entries.first() else {
        return Err(anyhow!(
            "no meshes were generated ({skipped} file(s) skipped)"
        ));
    };

    fs::create_dir_all(&cli.out)
        .with_context(|| format!("cannot create output directory {}", cli.out.display()))?;

    if matches!(cli.lang, Lang::C | Lang::Both | Lang::All) {
        let (header, source) = generate(&cli.name, &entries)?;
        let header_path = cli.out.join(format!("{}.h", cli.name));
        let source_path = cli.out.join(format!("{}.c", cli.name));

        fs::write(&header_path, header)
            .with_context(|| format!("cannot write {}", header_path.display()))?;
        fs::write(&source_path, source)
            .with_context(|| format!("cannot write {}", source_path.display()))?;
    }

    if matches!(cli.lang, Lang::Rust | Lang::Both | Lang::All) {
        let source = generate_rust(&cli.name, &entries)?;
        let source_path = cli.out.join(format!("{}.rs", cli.name));

        fs::write(&source_path, source)
            .with_context(|| format!("cannot write {}", source_path.display()))?;
    }

    if matches!(cli.lang, Lang::Obj | Lang::All) {
        let source = obj::render(&entries);
        let source_path = cli.out.join(format!("{}.obj", cli.name));

        fs::write(&source_path, source)
            .with_context(|| format!("cannot write {}", source_path.display()))?;
    }

    if !cli.quiet {
        println!(
            "generated {} meshes ({skipped} skipped) to {}",
            entries.len(),
            cli.out.display()
        );
    }

    Ok(())
}

fn load_mesh(path: &Path, cli: &Cli) -> Result<GlyphMesh> {
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow!("file name has no stem"))?;

    let text =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let segments = parse_document(&text)?;

    let mesh = if cli.three_d {
        Mesh::Three(build_3d(&segments, cli.tolerance, cli.depth)?)
    } else {
        Mesh::Two(build_2d(&segments, cli.tolerance)?)
    };

    Ok(GlyphMesh {
        code_point: code_point_from_name(&name),
        name,
        mesh,
    })
}

fn svg_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let reader = fs::read_dir(directory)
        .with_context(|| format!("cannot read directory {}", directory.display()))?;

    let mut paths = Vec::new();
    for entry in reader {
        let entry = entry.with_context(|| format!("cannot read {}", directory.display()))?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
        {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn code_point_from_name(name: &str) -> Option<u32> {
    let digits = name
        .strip_prefix("U+")
        .or_else(|| name.strip_prefix("u+"))?;
    if digits.is_empty() {
        return None;
    }
    u32::from_str_radix(digits, 16)
        .ok()
        .filter(|code_point| *code_point <= 0x10_FFFF)
}
