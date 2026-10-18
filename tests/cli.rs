use std::path::{Path, PathBuf};

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use tempfile::TempDir;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/Silkscreen-Regular.ttf")
}

fn temp_out() -> TempDir {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
    std::fs::create_dir_all(&base).expect("create test temp directory");
    tempfile::Builder::new()
        .prefix("ttf2svg-")
        .tempdir_in(&base)
        .expect("create temporary directory")
}

fn command() -> assert_cmd::Command {
    cargo_bin_cmd!("ttf2svg")
}

#[test]
fn exports_a_range() {
    let out = temp_out();
    let dir = out.path().join("svg");

    command()
        .arg(fixture())
        .arg("-r")
        .arg("0x0041-0x0043")
        .arg("-o")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("exported 3 glyphs"));

    for name in ["U+0041.svg", "U+0042.svg", "U+0043.svg"] {
        let path = dir.join(name);
        assert!(path.is_file(), "missing {}", path.display());
    }

    let svg = std::fs::read_to_string(dir.join("U+0041.svg")).expect("read svg");
    assert!(
        svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""),
        "{svg}"
    );
    assert!(svg.contains("<path d=\"M"), "{svg}");
    assert!(svg.contains("fill=\"currentColor\""), "{svg}");
}

#[test]
fn merges_overlapping_ranges() {
    let out = temp_out();
    let dir = out.path().join("svg");

    command()
        .arg(fixture())
        .arg("-r")
        .arg("0x41-0x43, 0x42-0x44")
        .arg("-o")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("exported 4 glyphs"));

    for name in ["U+0041.svg", "U+0042.svg", "U+0043.svg", "U+0044.svg"] {
        assert!(dir.join(name).is_file(), "missing {name}");
    }
}

#[test]
fn output_is_deterministic() {
    let out = temp_out();
    let first = out.path().join("first");
    let second = out.path().join("second");

    for dir in [&first, &second] {
        command()
            .arg(fixture())
            .arg("-r")
            .arg("0x0041")
            .arg("-o")
            .arg(dir)
            .assert()
            .success();
    }

    let a = std::fs::read(first.join("U+0041.svg")).expect("first svg");
    let b = std::fs::read(second.join("U+0041.svg")).expect("second svg");
    assert_eq!(a, b);
}

#[test]
fn rejects_invalid_range_with_exit_code_2() {
    let out = temp_out();

    command()
        .arg(fixture())
        .arg("-r")
        .arg("0x5A-0x41")
        .arg("-o")
        .arg(out.path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("greater than"));
}

#[test]
fn rejects_missing_font_with_exit_code_1() {
    let out = temp_out();

    command()
        .arg(out.path().join("does-not-exist.ttf"))
        .arg("-r")
        .arg("0x41")
        .arg("-o")
        .arg(out.path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("cannot read font file"));
}

#[test]
fn reports_unmapped_code_points() {
    let out = temp_out();

    command()
        .arg(fixture())
        .arg("-r")
        .arg("0x0041, 0x0378")
        .arg("-o")
        .arg(out.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("1 unmapped"))
        .stderr(predicate::str::contains("not present in the font"));
}

#[test]
fn fails_when_nothing_was_exported() {
    let out = temp_out();

    command()
        .arg(fixture())
        .arg("-r")
        .arg("0x0378")
        .arg("-o")
        .arg(out.path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("no glyphs were exported"));
}

#[test]
fn quiet_suppresses_the_summary() {
    let out = temp_out();

    command()
        .arg(fixture())
        .arg("-r")
        .arg("0x0041")
        .arg("-o")
        .arg(out.path())
        .arg("-q")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn rejects_woff_input() {
    let out = temp_out();
    let fake = out.path().join("font.woff");
    std::fs::write(&fake, b"wOFFjunk").expect("write fake woff");

    command()
        .arg(&fake)
        .arg("-r")
        .arg("0x41")
        .arg("-o")
        .arg(out.path().join("svg"))
        .assert()
        .code(1)
        .stderr(predicate::str::contains("WOFF"));
}
