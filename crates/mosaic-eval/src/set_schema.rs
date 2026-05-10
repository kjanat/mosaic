//! Schema for `#set` directives recognised in MVP 1.5.
//!
//! Each [`Target`] (e.g. `page`, `text`, `document`) advertises a fixed
//! list of `(key, slot type)` pairs. Unknown targets and unknown keys
//! produce diagnostics in the lowerer (`E020`/`E021`). Slot types drive
//! the `coerce_value` step and `E022` type-mismatch messages.

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum Target {
    Page,
    Text,
    Document,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum SlotType {
    Str,
    Float,
    Length,
}

impl SlotType {
    pub(crate) fn expected(self) -> &'static str {
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

pub(crate) fn lookup_target(name: &str) -> Option<Target> {
    match name {
        "page" => Some(Target::Page),
        "text" => Some(Target::Text),
        "document" => Some(Target::Document),
        _ => None,
    }
}

impl Target {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Text => "text",
            Self::Document => "document",
        }
    }

    fn slots(self) -> &'static [Slot] {
        match self {
            Self::Page => PAGE_SLOTS,
            Self::Text => TEXT_SLOTS,
            Self::Document => DOCUMENT_SLOTS,
        }
    }

    pub(crate) fn slot(self, key: &str) -> Option<SlotType> {
        self.slots().iter().find(|s| s.key == key).map(|s| s.ty)
    }

    pub(crate) fn keys(self) -> Vec<&'static str> {
        self.slots().iter().map(|s| s.key).collect()
    }
}
