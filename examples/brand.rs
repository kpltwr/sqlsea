//! Builds the brand kit from `brand/source`: `cargo run --example brand`.
//!
//! Writes every source to `brand/svg` with its text turned into outlines, so
//! the files render the same on machines without the fonts, and renders the
//! PNGs into `brand/png`.

use std::fs;
use std::path::Path;

const FONTS: [&[u8]; 5] = [
    include_bytes!("../assets/fonts/IBMPlexSans-400.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-500.ttf"),
    include_bytes!("../assets/fonts/FamiljenGrotesk-500.ttf"),
    include_bytes!("../assets/fonts/FamiljenGrotesk-600.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
];

/// Source name, then the pixel widths to render it at. Icons get square files
/// named by size; everything else is named by width.
const RENDERS: &[(&str, &[u32])] = &[
    ("icon-small", &[16, 24, 32]),
    ("icon", &[48, 64, 128, 256, 512, 1024]),
    ("mark", &[292, 584]),
    ("mark-mono", &[292, 584]),
    ("wordmark", &[336, 672]),
    ("lockup", &[528, 1056]),
    ("lockup-dark", &[528, 1056]),
    ("lockup-mono", &[528, 1056]),
    ("sheet", &[1600]),
    ("social", &[1280]),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("brand");
    fs::create_dir_all(root.join("svg"))?;
    fs::create_dir_all(root.join("png"))?;

    let mut options = resvg::usvg::Options::default();
    for font in FONTS {
        options.fontdb_mut().load_font_data(font.to_vec());
    }

    for (name, widths) in RENDERS {
        let source = fs::read_to_string(root.join("source").join(format!("{name}.svg")))?;
        let tree = resvg::usvg::Tree::from_str(&source, &options)?;

        let outlined = tree.to_string(&resvg::usvg::WriteOptions::default());
        fs::write(root.join("svg").join(format!("{name}.svg")), outlined)?;

        let size = tree.size();
        for &width in *widths {
            let scale = width as f32 / size.width();
            let height = (size.height() * scale).round() as u32;
            let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).ok_or("empty image")?;
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_scale(scale, scale),
                &mut pixmap.as_mut(),
            );
            let file = if name.starts_with("icon") {
                format!("icon-{width}.png")
            } else {
                format!("{name}-{width}.png")
            };
            pixmap.save_png(root.join("png").join(&file))?;
            println!("brand/png/{file}");
        }
    }
    Ok(())
}
