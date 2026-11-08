use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree};
use std::path::PathBuf;

fn render(tree: &Tree, size: u32) -> Pixmap {
    let mut pixmap = Pixmap::new(size, size).unwrap();
    let scale = size as f32 / tree.size().width();
    resvg::render(
        tree,
        Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
}

fn main() {
    println!("cargo:rerun-if-changed=icon.svg");
    let svg = std::fs::read("icon.svg").unwrap();
    let tree = Tree::from_data(&svg, &Options::default()).unwrap();
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let rgba: Vec<u8> = render(&tree, 256)
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    std::fs::write(out.join("icon.rgba"), rgba).unwrap();
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let ico = out.join("icon.ico");
        std::fs::write(&ico, icon_file(&tree)).unwrap();
        #[cfg(windows)]
        winresource::WindowsResource::new()
            .set_icon(ico.to_str().unwrap())
            .compile()
            .unwrap();
    }
}

fn icon_file(tree: &Tree) -> Vec<u8> {
    let sizes = [16, 24, 32, 48, 64, 256];
    let images: Vec<Vec<u8>> = sizes
        .iter()
        .map(|&s| render(tree, s).encode_png().unwrap())
        .collect();
    let mut file = vec![0, 0, 1, 0, sizes.len() as u8, 0];
    let mut offset = 6 + 16 * sizes.len();
    for (size, image) in sizes.iter().zip(&images) {
        let side = if *size == 256 { 0 } else { *size as u8 };
        file.extend([side, side, 0, 0, 1, 0, 32, 0]);
        file.extend((image.len() as u32).to_le_bytes());
        file.extend((offset as u32).to_le_bytes());
        offset += image.len();
    }
    images.iter().for_each(|image| file.extend(image));
    file
}
