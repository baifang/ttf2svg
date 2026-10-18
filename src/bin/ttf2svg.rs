#![forbid(unsafe_code)]

//! `ttf2svg`: export selected glyphs of a font as individual SVG files.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};
use clap::Parser;

use ttf2svg::font::Font;
use ttf2svg::range::{parse_specs, CodePointRange};
use ttf2svg::svg::render;

#[derive(Debug, Parser)]
#[command(
    name = "ttf2svg",
    version,
    about = "Export glyphs from a TrueType/OpenType font as individual SVG files",
    after_help = "Examples:\n  ttf2svg font.ttf -r \"0x0041-0x005A, 0x0061-0x007A\" -o out\n  ttf2svg font.ttf -r 0x4E00-0x9FFF -o han"
)]
struct Cli {
    /// Input .ttf or .otf font file
    #[arg(value_name = "FONT")]
    font: PathBuf,

    /// Unicode code point ranges to export, e.g. "0x41-0x5A, 0x61"
    #[arg(short = 'r', long = "range", value_name = "SPEC", required = true)]
    ranges: Vec<String>,

    /// Output directory; created if missing
    #[arg(short = 'o', long = "out", value_name = "DIR", default_value = "out")]
    out: PathBuf,

    /// Suppress warnings and the summary line
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let ranges = match parse_specs(&cli.ranges) {
        Ok(ranges) => ranges,
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!("try 'ttf2svg --help' for usage");
            return ExitCode::from(2);
        }
    };

    match run(&cli, &ranges) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli, ranges: &[CodePointRange]) -> Result<()> {
    let data = fs::read(&cli.font)
        .with_context(|| format!("cannot read font file {}", cli.font.display()))?;
    let font = Font::parse(&data, &cli.font)?;

    fs::create_dir_all(&cli.out)
        .with_context(|| format!("cannot create output directory {}", cli.out.display()))?;

    let mut exported = 0_usize;
    let mut unmapped = 0_usize;
    let mut empty = 0_usize;
    let mut unmapped_examples = Vec::new();

    for range in ranges {
        for code_point in range.start..=range.end {
            let Some(outline) = font.outline(code_point) else {
                unmapped += 1;
                if unmapped_examples.len() < 5 {
                    unmapped_examples.push(code_point);
                }
                continue;
            };

            if outline.is_empty() {
                empty += 1;
                continue;
            }

            let Some(svg) = render(&outline) else {
                empty += 1;
                continue;
            };

            let path = cli.out.join(format!("U+{code_point:04X}.svg"));
            fs::write(&path, svg).with_context(|| format!("cannot write {}", path.display()))?;
            exported += 1;
        }
    }

    if !cli.quiet && !unmapped_examples.is_empty() {
        let examples = unmapped_examples
            .iter()
            .map(|code_point| format!("U+{code_point:04X}"))
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!("warning: {unmapped} code point(s) not present in the font (first: {examples})");
    }

    if exported == 0 {
        return Err(anyhow!(
            "no glyphs were exported ({unmapped} unmapped, {empty} empty)"
        ));
    }

    if !cli.quiet {
        println!(
            "exported {exported} glyphs ({unmapped} unmapped, {empty} empty) to {}",
            cli.out.display()
        );
    }

    Ok(())
}
