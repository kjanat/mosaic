use std::borrow::Cow;

use unicode_normalization::{UnicodeNormalization, is_nfc_quick};

/// Return `text` in Unicode NFC form, borrowing when it is already normalized.
///
/// # Examples
///
/// ```
/// use mos_fonts::nfc_text;
///
/// assert_eq!(nfc_text("S\u{0326}"), "\u{0218}");
/// ```
#[must_use]
pub fn nfc_text(text: &str) -> Cow<'_, str> {
    if is_nfc_quick(text.chars()) == unicode_normalization::IsNormalized::Yes {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.nfc().collect())
    }
}
