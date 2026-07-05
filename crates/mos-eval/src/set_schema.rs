//! Schema for `#set` directives recognised in MVP 1.5.
//!
//! Each [`Target`] (e.g. `page`, `text`, `document`) advertises a fixed
//! list of `(key, slot type)` pairs. Unknown targets and unknown keys
//! produce diagnostics in the lowerer (`MOS0011`/`MOS0015`). Slot types drive
//! the `coerce_value` step and `MOS0020` type-mismatch messages.

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Target {
    Page,
    Text,
    Document,
    /// Default styling for `#image(...)` calls; currently just `width`
    /// and `height` are recognised. The MVP 1.5 emit path doesn't yet
    /// pick these defaults up on bare images (only explicit per-call
    /// width/height apply), but accepting them in the schema keeps
    /// `#set image(width: ...)` from emitting MOS0011 today.
    Image,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SlotType {
    Str,
    Float,
    Length,
}

impl SlotType {
    /// Return the user-facing type description used in diagnostics.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_eval::set_schema::SlotType;
    ///
    /// assert_eq!(SlotType::Length.expected(), "a length (e.g. 24mm, 11pt)");
    /// ```
    #[must_use]
    pub const fn expected(self) -> &'static str {
        match self {
            Self::Str => "a string",
            Self::Float => "a number",
            Self::Length => "a length (e.g. 24mm, 11pt)",
        }
    }
}

struct Slot {
    key: &'static str,
    ty: SlotType,
}

const PAGE_SLOTS: &[Slot] = &[
    Slot {
        key: "paper",
        ty: SlotType::Str,
    },
    Slot {
        key: "margin",
        ty: SlotType::Length,
    },
    Slot {
        key: "numbering",
        ty: SlotType::Str,
    },
];

const TEXT_SLOTS: &[Slot] = &[
    Slot {
        key: "font",
        ty: SlotType::Str,
    },
    Slot {
        key: "size",
        ty: SlotType::Length,
    },
    Slot {
        key: "leading",
        ty: SlotType::Float,
    },
];

const DOCUMENT_SLOTS: &[Slot] = &[
    Slot {
        key: "title",
        ty: SlotType::Str,
    },
    Slot {
        key: "author",
        ty: SlotType::Str,
    },
    Slot {
        key: "language",
        ty: SlotType::Str,
    },
];

const IMAGE_SLOTS: &[Slot] = &[
    Slot {
        key: "width",
        ty: SlotType::Length,
    },
    Slot {
        key: "height",
        ty: SlotType::Length,
    },
];

/// Single source of truth pairing each `#set` target spelling with its
/// [`Target`]; drives both [`lookup_target`] and the MOS0011 nearest-match
/// candidate set.
const TARGETS: &[(&str, Target)] = &[
    ("page", Target::Page),
    ("text", Target::Text),
    ("document", Target::Document),
    ("image", Target::Image),
];

#[must_use]
pub fn lookup_target(name: &str) -> Option<Target> {
    TARGETS
        .iter()
        .find(|(spelling, _)| *spelling == name)
        .map(|&(_, target)| target)
}

/// All `#set` target spellings, in declaration order; the candidate set for
/// MOS0011 "did you mean" hints.
pub(crate) fn target_names() -> impl Iterator<Item = &'static str> {
    TARGETS.iter().map(|&(spelling, _)| spelling)
}

impl Target {
    /// Return the source spelling for this `#set` target.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_eval::set_schema::Target;
    ///
    /// assert_eq!(Target::Text.name(), "text");
    /// ```
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Text => "text",
            Self::Document => "document",
            Self::Image => "image",
        }
    }

    const fn slots(self) -> &'static [Slot] {
        match self {
            Self::Page => PAGE_SLOTS,
            Self::Text => TEXT_SLOTS,
            Self::Document => DOCUMENT_SLOTS,
            Self::Image => IMAGE_SLOTS,
        }
    }

    /// Look up the expected slot type for a key on this target.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_eval::set_schema::{SlotType, Target};
    ///
    /// assert_eq!(Target::Text.slot("size"), Some(SlotType::Length));
    /// assert_eq!(Target::Text.slot("paper"), None);
    /// ```
    #[must_use]
    pub fn slot(self, key: &str) -> Option<SlotType> {
        self.slots().iter().find(|s| s.key == key).map(|s| s.ty)
    }

    /// Return all accepted keys for this target in schema order.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_eval::set_schema::Target;
    ///
    /// assert_eq!(Target::Page.keys(), vec!["paper", "margin", "numbering"]);
    /// ```
    #[must_use]
    pub fn keys(self) -> Vec<&'static str> {
        self.slots().iter().map(|s| s.key).collect()
    }
}
