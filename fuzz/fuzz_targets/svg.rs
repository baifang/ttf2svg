#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(segments) = ttf2svg::svgpath::parse_document(text) else {
        return;
    };
    let _ = ttf2svg::mesh::build_2d(&segments, 0.5);
    let _ = ttf2svg::mesh::build_3d(&segments, 0.5, 10.0);
});
