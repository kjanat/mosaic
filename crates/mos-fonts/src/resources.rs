use std::sync::LazyLock;

use crate::{EmbeddedFont, EmbeddedFontId};

/// Return bundled font data for `id`.
#[must_use]
pub fn embedded_font(id: EmbeddedFontId) -> &'static EmbeddedFont {
    match id {
        EmbeddedFontId::Regular => &NOTO_SANS_REGULAR,
        EmbeddedFontId::Bold => &NOTO_SANS_BOLD,
        EmbeddedFontId::Italic => &NOTO_SANS_ITALIC,
        EmbeddedFontId::BoldItalic => &NOTO_SANS_BOLD_ITALIC,
        EmbeddedFontId::Mono => &NOTO_SANS_MONO,
        EmbeddedFontId::Math => &NOTO_SANS_MATH,
    }
}

/// Return the PDF resource name for embedded font `id`.
#[must_use]
pub const fn pdf_resource_name(id: EmbeddedFontId) -> &'static [u8] {
    RESOURCE_NAMES[id.pdf_resource_index() as usize]
}

// Baked at build time by `build.rs`: 256 entries `b"F0"`..`b"F255"`
// indexed by `EmbeddedFontId::pdf_resource_index`.
include!(concat!(env!("OUT_DIR"), "/resource_names.rs"));

static NOTO_SANS_REGULAR: LazyLock<EmbeddedFont> = LazyLock::new(|| {
    EmbeddedFont::from_static(
        include_bytes!("../data/NotoSans-Regular.ttf"),
        "NotoSans",
        false,
        false,
    )
});

static NOTO_SANS_BOLD: LazyLock<EmbeddedFont> = LazyLock::new(|| {
    EmbeddedFont::from_static(
        include_bytes!("../data/NotoSans-Bold.ttf"),
        "NotoSans-Bold",
        true,
        false,
    )
});

static NOTO_SANS_ITALIC: LazyLock<EmbeddedFont> = LazyLock::new(|| {
    EmbeddedFont::from_static(
        include_bytes!("../data/NotoSans-Italic.ttf"),
        "NotoSans-Italic",
        false,
        true,
    )
});

static NOTO_SANS_BOLD_ITALIC: LazyLock<EmbeddedFont> = LazyLock::new(|| {
    EmbeddedFont::from_static(
        include_bytes!("../data/NotoSans-BoldItalic.ttf"),
        "NotoSans-BoldItalic",
        true,
        true,
    )
});

static NOTO_SANS_MONO: LazyLock<EmbeddedFont> = LazyLock::new(|| {
    EmbeddedFont::from_static(
        include_bytes!("../data/NotoSansMono-Regular.ttf"),
        "NotoSansMono",
        false,
        false,
    )
});

static NOTO_SANS_MATH: LazyLock<EmbeddedFont> = LazyLock::new(|| {
    EmbeddedFont::from_static(
        include_bytes!("../data/NotoSansMath-Regular.ttf"),
        "NotoSansMath",
        false,
        false,
    )
});
