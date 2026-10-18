use std::path::{Path, PathBuf};

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use tempfile::TempDir;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/Silkscreen-Regular.ttf")
}

fn temp_dir() -> TempDir {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
    std::fs::create_dir_all(&base).expect("create test temp directory");
    tempfile::Builder::new()
        .prefix("svg2mesh-")
        .tempdir_in(&base)
        .expect("create temporary directory")
}

fn write_svg(directory: &Path, name: &str, path_data: &str) {
    let body = format!("<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"{path_data}\"/></svg>");
    std::fs::write(directory.join(name), body).expect("write svg");
}

#[test]
fn generates_2d_mesh_files() {
    let tmp = temp_dir();
    let input = tmp.path().join("svg");
    let output = tmp.path().join("generated");
    std::fs::create_dir_all(&input).expect("create input dir");
    write_svg(&input, "U+0041.svg", "M0 0 H10 V10 H0 Z");

    cargo_bin_cmd!("svg2mesh")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("-n")
        .arg("glyphs")
        .assert()
        .success()
        .stdout(predicate::str::contains("generated 1 meshes"));

    let header = std::fs::read_to_string(output.join("glyphs.h")).expect("header");
    let source = std::fs::read_to_string(output.join("glyphs.c")).expect("source");

    assert!(
        header.contains("#define SVG_MESH_DIMENSIONS 2u"),
        "{header}"
    );
    assert!(header.contains("typedef struct"), "{header}");
    assert!(source.contains("#include \"glyphs.h\""), "{source}");
    assert!(
        source.contains("static const float svg_mesh_0_vertices[]"),
        "{source}"
    );
    assert!(
        source.contains("static const uint16_t svg_mesh_0_indices[]"),
        "{source}"
    );
    assert!(source.contains("\"U+0041\""), "{source}");
    assert!(source.contains("0x41u"), "{source}");
}

#[test]
fn generates_3d_meshes_with_normals() {
    let tmp = temp_dir();
    let input = tmp.path().join("svg");
    let output = tmp.path().join("generated");
    std::fs::create_dir_all(&input).expect("create input dir");
    write_svg(&input, "square.svg", "M0 0 H10 V10 H0 Z");

    cargo_bin_cmd!("svg2mesh")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("--3d")
        .arg("--depth")
        .arg("5")
        .assert()
        .success();

    let header = std::fs::read_to_string(output.join("glyphs.h")).expect("header");
    let source = std::fs::read_to_string(output.join("glyphs.c")).expect("source");

    assert!(
        header.contains("#define SVG_MESH_DIMENSIONS 3u"),
        "{header}"
    );
    assert!(
        source.contains("static const float svg_mesh_0_normals[]"),
        "{source}"
    );
    assert!(source.contains("\"square\""), "{source}");
}

#[test]
fn rejects_a_directory_without_svgs() {
    let tmp = temp_dir();

    cargo_bin_cmd!("svg2mesh")
        .arg(tmp.path())
        .arg("-o")
        .arg(tmp.path().join("generated"))
        .assert()
        .code(1)
        .stderr(predicate::str::contains("no .svg files"));
}

#[test]
fn skips_unparsable_files_and_fails_when_empty() {
    let tmp = temp_dir();
    let input = tmp.path().join("svg");
    std::fs::create_dir_all(&input).expect("create input dir");
    write_svg(&input, "broken.svg", "M0 0 K1 1");

    cargo_bin_cmd!("svg2mesh")
        .arg(&input)
        .arg("-o")
        .arg(tmp.path().join("generated"))
        .assert()
        .code(1)
        .stderr(predicate::str::contains("skipping"))
        .stderr(predicate::str::contains("no meshes were generated"));
}

#[test]
fn converts_ttf2svg_output_end_to_end() {
    let tmp = temp_dir();
    let svg_dir = tmp.path().join("svg");
    let mesh_dir = tmp.path().join("generated");

    cargo_bin_cmd!("ttf2svg")
        .arg(fixture())
        .arg("-r")
        .arg("0x0041-0x0042")
        .arg("-o")
        .arg(&svg_dir)
        .assert()
        .success();

    cargo_bin_cmd!("svg2mesh")
        .arg(&svg_dir)
        .arg("-o")
        .arg(&mesh_dir)
        .assert()
        .success();

    let header = std::fs::read_to_string(mesh_dir.join("glyphs.h")).expect("header");
    let source = std::fs::read_to_string(mesh_dir.join("glyphs.c")).expect("source");

    assert!(header.contains("#define SVG_MESH_COUNT 2u"), "{header}");
    assert!(source.contains("\"U+0041\""), "{source}");
    assert!(source.contains("\"U+0042\""), "{source}");
}

#[test]
fn generates_a_rust_module() {
    let tmp = temp_dir();
    let input = tmp.path().join("svg");
    let output = tmp.path().join("generated");
    std::fs::create_dir_all(&input).expect("create input dir");
    write_svg(&input, "U+0041.svg", "M0 0 H10 V10 H0 Z");

    cargo_bin_cmd!("svg2mesh")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("--lang")
        .arg("rust")
        .assert()
        .success();

    assert!(!output.join("glyphs.h").exists());
    assert!(!output.join("glyphs.c").exists());

    let source = std::fs::read_to_string(output.join("glyphs.rs")).expect("rust source");
    assert!(source.contains("pub struct Mesh"), "{source}");
    assert!(
        source.contains("pub const DIMENSIONS: usize = 2;"),
        "{source}"
    );
    assert!(source.contains("pub static MESHES: &[Mesh]"), "{source}");
    assert!(source.contains("pub fn find(code_point: u32)"), "{source}");
    assert!(source.contains("\"U+0041\""), "{source}");
}

#[test]
fn generates_both_languages() {
    let tmp = temp_dir();
    let input = tmp.path().join("svg");
    let output = tmp.path().join("generated");
    std::fs::create_dir_all(&input).expect("create input dir");
    write_svg(&input, "square.svg", "M0 0 H10 V10 H0 Z");

    cargo_bin_cmd!("svg2mesh")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("--lang")
        .arg("both")
        .arg("--3d")
        .assert()
        .success();

    for name in ["glyphs.h", "glyphs.c", "glyphs.rs"] {
        assert!(output.join(name).is_file(), "missing {name}");
    }

    let source = std::fs::read_to_string(output.join("glyphs.rs")).expect("rust source");
    assert!(
        source.contains("pub const DIMENSIONS: usize = 3;"),
        "{source}"
    );
    assert!(source.contains("Some(&MESH_0_NORMALS)"), "{source}");
}

#[test]
fn generates_obj_only() {
    let tmp = temp_dir();
    let input = tmp.path().join("svg");
    let output = tmp.path().join("generated");
    std::fs::create_dir_all(&input).expect("create input dir");
    write_svg(&input, "U+0041.svg", "M0 0 H10 V10 H0 Z");

    cargo_bin_cmd!("svg2mesh")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("--lang")
        .arg("obj")
        .assert()
        .success();

    assert!(!output.join("glyphs.h").exists());
    assert!(!output.join("glyphs.c").exists());
    assert!(!output.join("glyphs.rs").exists());

    let obj = std::fs::read_to_string(output.join("glyphs.obj")).expect("obj");
    assert!(obj.contains("o U+0041"), "{obj}");
    assert!(obj.lines().any(|line| line.starts_with("v ")), "{obj}");
    assert!(obj.lines().any(|line| line.starts_with("f ")), "{obj}");
    assert!(!obj.contains("vn "), "{obj}");
}

#[test]
fn generates_all_outputs() {
    let tmp = temp_dir();
    let input = tmp.path().join("svg");
    let output = tmp.path().join("generated");
    std::fs::create_dir_all(&input).expect("create input dir");
    write_svg(&input, "square.svg", "M0 0 H10 V10 H0 Z");

    cargo_bin_cmd!("svg2mesh")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("--lang")
        .arg("all")
        .arg("--3d")
        .assert()
        .success();

    for name in ["glyphs.h", "glyphs.c", "glyphs.rs", "glyphs.obj"] {
        assert!(output.join(name).is_file(), "missing {name}");
    }

    let obj = std::fs::read_to_string(output.join("glyphs.obj")).expect("obj");
    assert!(obj.contains("o square"), "{obj}");
    assert!(obj.contains("vn "), "{obj}");
    assert!(
        obj.lines()
            .any(|line| line.starts_with("f ") && line.contains("//1")),
        "{obj}"
    );
}
