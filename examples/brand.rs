//! Builds the brand kit from `brand/source`: `cargo run --example brand`.
//!
//! Writes every source to `brand/svg` with its text turned into outlines, so
//! the files render the same on machines without the fonts, renders the PNGs
//! into `brand/png`, and packs the app icon as `brand/icon.ico` (Windows) and
//! `brand/icon.icns` (macOS).

use std::collections::BTreeMap;
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

    // App icon renders by size, for the Windows and macOS icon files.
    let mut icons: BTreeMap<u32, Vec<u8>> = BTreeMap::new();

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
            let png = pixmap.encode_png()?;
            fs::write(root.join("png").join(&file), &png)?;
            if name.starts_with("icon") {
                icons.insert(width, png);
            }
            println!("brand/png/{file}");
        }
    }

    fs::write(
        root.join("icon.ico"),
        ico(&icons, &[16, 24, 32, 48, 64, 128, 256]),
    )?;
    println!("brand/icon.ico");
    fs::write(root.join("icon.icns"), icns(&icons))?;
    println!("brand/icon.icns");
    Ok(())
}

/// A Windows icon holding PNG images, which Windows reads at every size.
fn ico(icons: &BTreeMap<u32, Vec<u8>>, sizes: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(0u16.to_le_bytes()); // reserved
    out.extend(1u16.to_le_bytes()); // type: icon
    out.extend((sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for size in sizes {
        let png = &icons[size];
        let side = if *size >= 256 { 0 } else { *size as u8 }; // 0 means 256
        out.extend([side, side, 0, 0]);
        out.extend(1u16.to_le_bytes()); // colour planes
        out.extend(32u16.to_le_bytes()); // bits per pixel
        out.extend((png.len() as u32).to_le_bytes());
        out.extend(offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for size in sizes {
        out.extend(&icons[size]);
    }
    out
}

/// A macOS icon: one PNG per slot, each slot a size at 1x or 2x.
fn icns(icons: &BTreeMap<u32, Vec<u8>>) -> Vec<u8> {
    const SLOTS: [(&[u8; 4], u32); 11] = [
        (b"icp4", 16),
        (b"icp5", 32),
        (b"icp6", 64),
        (b"ic07", 128),
        (b"ic08", 256),
        (b"ic09", 512),
        (b"ic10", 1024), // 512 at 2x
        (b"ic11", 32),   // 16 at 2x
        (b"ic12", 64),   // 32 at 2x
        (b"ic13", 256),  // 128 at 2x
        (b"ic14", 512),  // 256 at 2x
    ];
    let mut body = Vec::new();
    for (kind, size) in SLOTS {
        let png = &icons[&size];
        body.extend(kind);
        body.extend((8 + png.len() as u32).to_be_bytes());
        body.extend(png);
    }
    let mut out = b"icns".to_vec();
    out.extend((8 + body.len() as u32).to_be_bytes());
    out.extend(body);
    out
}
