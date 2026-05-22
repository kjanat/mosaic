//! Zed extension entrypoint for Mosaic language support.

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

use zed_extension_api as zed;

#[derive(Debug)]
struct MosaicExtension;

impl zed::Extension for MosaicExtension {
    fn new() -> Self {
        Self
    }
}

zed::register_extension!(MosaicExtension);
