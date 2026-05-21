//! Zed extension entrypoint for Mosaic language support.

use zed_extension_api as zed;

#[derive(Debug)]
struct MosaicExtension;

impl zed::Extension for MosaicExtension {
    fn new() -> Self {
        Self
    }
}

zed::register_extension!(MosaicExtension);
