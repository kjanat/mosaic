# Bundled font data sources

All four Noto Sans cuts shipped under this directory were downloaded
from the upstream `notofonts/notofonts.github.io` repository
(`main` branch) on 2026-05-11. The license text under
`LICENSE-Noto-Sans` was downloaded from
`notofonts/latin-greek-cyrillic`, which is the upstream package for
the Noto Sans Latin/Greek/Cyrillic build that ships in the
`notofonts.github.io` mirror.

Files are vendored verbatim — no re-subsetting, no metadata edits. The
PDF backend re-subsets per emitted document.

## Files

| File                     | Upstream URL                                                                                                              | SHA-256                                                            |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| `NotoSans-Regular.ttf`    | `https://raw.githubusercontent.com/notofonts/notofonts.github.io/main/fonts/NotoSans/full/ttf/NotoSans-Regular.ttf`        | `f5f552c8c5edb61fe6efb824baf4d4de47b1a8689ab4925ff43f7bd6a4ebece5` |
| `NotoSans-Bold.ttf`       | `https://raw.githubusercontent.com/notofonts/notofonts.github.io/main/fonts/NotoSans/full/ttf/NotoSans-Bold.ttf`           | `3a08a47daa00cade516425c15c57615aef2fd418ec9811a7b9f465088f92cc05` |
| `NotoSans-Italic.ttf`     | `https://raw.githubusercontent.com/notofonts/notofonts.github.io/main/fonts/NotoSans/full/ttf/NotoSans-Italic.ttf`         | `126522ae1bb9cd92120287fc47dfc74ef981e73931d93e52c565fb7e09b2d74a` |
| `NotoSans-BoldItalic.ttf` | `https://raw.githubusercontent.com/notofonts/notofonts.github.io/main/fonts/NotoSans/full/ttf/NotoSans-BoldItalic.ttf`     | `2e34b41a4b9c234b1be7dff6d06cba18811ecb694b41350873edf0ec16a0f0fa` |
| `LICENSE-Noto-Sans`       | `https://raw.githubusercontent.com/notofonts/latin-greek-cyrillic/main/OFL.txt`                                            | `cee9892f9f0cc8fe882c9e9537ee6a89621d86ee7ceaf70b02e2b2b1c25c061a` |

To refresh, re-download from the URLs above and update the hashes here.
The OFL.txt has no `with Reserved Font Name` clause attached to the
copyright line, so the SPDX expression `OFL-1.1` (not `OFL-1.1-RFN`) is
the correct identifier.
