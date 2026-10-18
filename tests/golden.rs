use std::path::{Path, PathBuf};

use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::TempDir;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/Silkscreen-Regular.ttf")
}

fn temp_dir() -> TempDir {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
    std::fs::create_dir_all(&base).expect("create test temp directory");
    tempfile::Builder::new()
        .prefix("golden-")
        .tempdir_in(&base)
        .expect("create temporary directory")
}

#[test]
fn glyph_svg_matches_the_golden_file() {
    let out = temp_dir();

    cargo_bin_cmd!("ttf2svg")
        .arg(fixture())
        .arg("-r")
        .arg("0x0041")
        .arg("-o")
        .arg(out.path())
        .assert()
        .success();

    let actual = std::fs::read_to_string(out.path().join("U+0041.svg")).expect("generated svg");
    let expected = include_str!("golden/U+0041.svg");

    assert_eq!(
        actual.replace("\r\n", "\n"),
        expected.replace("\r\n", "\n"),
        "the SVG output format changed; update tests/golden/U+0041.svg if intended"
    );
}
