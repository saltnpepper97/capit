// Author: Dustin Pilgrim
// License: MIT

use once_cell::sync::OnceCell;
use resvg::usvg;
use tiny_skia::Pixmap;

pub(crate) const ICON_SZ: i32 = 32;

// Embed SVGs relative to this module file (src/bar/)
const ICON_REGION_SVG: &[u8] = include_bytes!("icons/region.svg");
const ICON_SCREEN_SVG: &[u8] = include_bytes!("icons/screen.svg");
const ICON_WINDOW_SVG: &[u8] = include_bytes!("icons/window.svg");

pub(crate) struct IconMasks {
    pub region: Vec<u8>,
    pub screen: Vec<u8>,
    pub window: Vec<u8>,
}

static ICONS: OnceCell<IconMasks> = OnceCell::new();

pub(crate) fn icons() -> &'static IconMasks {
    ICONS.get_or_init(|| {
        let px = ICON_SZ as u32;

        let region = svg_alpha_mask(ICON_REGION_SVG, px)
            .unwrap_or_else(|_| vec![0; (ICON_SZ * ICON_SZ) as usize]);
        let screen = svg_alpha_mask(ICON_SCREEN_SVG, px)
            .unwrap_or_else(|_| vec![0; (ICON_SZ * ICON_SZ) as usize]);
        let window = svg_alpha_mask(ICON_WINDOW_SVG, px)
            .unwrap_or_else(|_| vec![0; (ICON_SZ * ICON_SZ) as usize]);

        IconMasks { region, screen, window }
    })
}

fn svg_alpha_mask(svg: &[u8], px: u32) -> Result<Vec<u8>, String> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg, &opt).map_err(|e| format!("usvg parse: {e:?}"))?;

    let mut pixmap = Pixmap::new(px, px).ok_or("tiny-skia pixmap alloc failed")?;

    // Scale SVG -> px x px
    let size = tree.size();
    let sx = px as f32 / size.width();
    let sy = px as f32 / size.height();
    let transform = tiny_skia::Transform::from_scale(sx, sy);

    // resvg render
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let data = pixmap.data();
    let mut mask = Vec::with_capacity((px * px) as usize);
    for i in 0..(px * px) as usize {
        mask.push(data[i * 4 + 3]); // alpha channel
    }
    Ok(mask)
}
