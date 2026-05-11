# Bundled font data sources

All bundled Noto Sans + Noto Sans Mono + Noto Sans Math cuts shipped under this directory were
downloaded from the upstream `notofonts/notofonts.github.io` repository.\
The four Noto Sans cuts and the Noto Sans Mono Regular cut were downloaded on 2026-05-11 from commit
[`28b15b4b43b7bed62b5cf6e6b0b5ff5846270535`] (2024-11-21).\
The Noto Sans Math Regular cut was downloaded on 2026-05-11 from commit
[`4a11562a772c8a536bd3bdc27ab660d0cf0f8ec0`] (2025).\
The license text under `LICENSE-Noto-Sans` was downloaded from `notofonts/latin-greek-cyrillic` at
commit [`4bc63d7ebca1faed49c6c685f380ba0abc2c1941`], the upstream package for the Noto Sans
Latin/Greek/Cyrillic build that ships in the `notofonts.github.io` mirror.\
The same OFL-1.1 license text covers Noto Sans Mono and Noto Sans Math.

Files are vendored verbatim — no re-subsetting, no metadata edits. The PDF backend re-subsets per
emitted document.

The URLs below are pinned to those upstream commit SHAs so re-vendoring fetches the exact same bytes
regardless of how the upstream `main` branch drifts. The per-file `SHA-256` column is the
load-bearing integrity check; the URLs are documentary. Note the Noto Sans cuts ship under
`fonts/NotoSans/full/ttf/` (a merged Latin/Greek/Cyrillic build), Noto Sans Mono ships under
`fonts/NotoSansMono/hinted/ttf/` (no `full/` path exists upstream for the mono family), and Noto
Sans Math ships under `fonts/NotoSansMath/full/ttf/` (a TrueType-outline build alongside the more
common CFF-OT `.otf` cut; we pick the TTF so it goes through the existing `/FontFile2` PDF embed
path).

## Files

| File                       | Upstream URL                                                                                                                                                      | SHA-256                                                            |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| `NotoSans-Regular.ttf`     | `https://raw.githubusercontent.com/notofonts/notofonts.github.io/28b15b4b43b7bed62b5cf6e6b0b5ff5846270535/fonts/NotoSans/full/ttf/NotoSans-Regular.ttf`           | `f5f552c8c5edb61fe6efb824baf4d4de47b1a8689ab4925ff43f7bd6a4ebece5` |
| `NotoSans-Bold.ttf`        | `https://raw.githubusercontent.com/notofonts/notofonts.github.io/28b15b4b43b7bed62b5cf6e6b0b5ff5846270535/fonts/NotoSans/full/ttf/NotoSans-Bold.ttf`              | `3a08a47daa00cade516425c15c57615aef2fd418ec9811a7b9f465088f92cc05` |
| `NotoSans-Italic.ttf`      | `https://raw.githubusercontent.com/notofonts/notofonts.github.io/28b15b4b43b7bed62b5cf6e6b0b5ff5846270535/fonts/NotoSans/full/ttf/NotoSans-Italic.ttf`            | `126522ae1bb9cd92120287fc47dfc74ef981e73931d93e52c565fb7e09b2d74a` |
| `NotoSans-BoldItalic.ttf`  | `https://raw.githubusercontent.com/notofonts/notofonts.github.io/28b15b4b43b7bed62b5cf6e6b0b5ff5846270535/fonts/NotoSans/full/ttf/NotoSans-BoldItalic.ttf`        | `2e34b41a4b9c234b1be7dff6d06cba18811ecb694b41350873edf0ec16a0f0fa` |
| `NotoSansMono-Regular.ttf` | `https://raw.githubusercontent.com/notofonts/notofonts.github.io/28b15b4b43b7bed62b5cf6e6b0b5ff5846270535/fonts/NotoSansMono/hinted/ttf/NotoSansMono-Regular.ttf` | `65b5e2b2c4a1fba9ae8be1f026cb35b03dcb8886d9b2a4147054fde12f7e767d` |
| `NotoSansMath-Regular.ttf` | `https://raw.githubusercontent.com/notofonts/notofonts.github.io/4a11562a772c8a536bd3bdc27ab660d0cf0f8ec0/fonts/NotoSansMath/full/ttf/NotoSansMath-Regular.ttf`   | `7283c396e9b22699bb542d9631030dc804a7e5b954f193d8f8f5b5f1162fbc61` |
| `LICENSE-Noto-Sans`        | `https://raw.githubusercontent.com/notofonts/latin-greek-cyrillic/4bc63d7ebca1faed49c6c685f380ba0abc2c1941/OFL.txt`                                               | `cee9892f9f0cc8fe882c9e9537ee6a89621d86ee7ceaf70b02e2b2b1c25c061a` |

To refresh, bump the commit SHAs above to a newer upstream and update the per-file hashes from the
re-downloaded bytes. The OFL.txt has no `with Reserved Font Name` clause attached to the copyright
line, so the SPDX expression `OFL-1.1` (not `OFL-1.1-RFN`) is the correct identifier.

[`4a11562a772c8a536bd3bdc27ab660d0cf0f8ec0`]: https://github.com/notofonts/notofonts.github.io/commit/4a11562a772c8a536bd3bdc27ab660d0cf0f8ec0
[`4bc63d7ebca1faed49c6c685f380ba0abc2c1941`]: https://github.com/notofonts/latin-greek-cyrillic/commit/4bc63d7ebca1faed49c6c685f380ba0abc2c1941
[`28b15b4b43b7bed62b5cf6e6b0b5ff5846270535`]: https://github.com/notofonts/notofonts.github.io/commit/28b15b4b43b7bed62b5cf6e6b0b5ff5846270535
