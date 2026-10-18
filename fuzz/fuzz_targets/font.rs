#![no_main]

use std::path::Path;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(font) = ttf2svg::font::Font::parse(data, Path::new("fuzz.ttf")) else {
        return;
    };
    for code_point in [0x20, 0x41, 0x4E00, 0x1F600] {
        let _ = font.outline(code_point);
    }
});
