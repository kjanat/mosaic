#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests panic loudly on setup failure; matches crate-wide test-module convention"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use mos_core::{AttrValue, CollectingSink, ContentHash, ContentHasher, Node, NodeKind};

use crate::image_lower::{
    BITS_PER_COMPONENT_ATTR, COLOR_SPACE_ATTR, PIXEL_HEIGHT_ATTR, PIXEL_WIDTH_ATTR, PIXELS_ATTR,
};
use crate::{Evaluator, LowerResult, lower};

const BIB: &str = "@book{one, title={First}}\n@book{two, title={Second}}\n";
const SOURCE: &str = "= Heading <heading>\n\n<paragraph> A *styled* paragraph [@one] @heading @page(heading).\n\n#image(\"image.png\", width: 20pt, alt: \"A picture\", label: \"image\")\n\n#figure(image: \"image.png\", caption: \"A caption\", label: \"figure\")\n\n#bibliography(\"refs.bib\")\n";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mos-semantic-hash-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = Self(dir);
        fixture.write_image([10, 20, 30]);
        std::fs::write(fixture.0.join("refs.bib"), BIB).unwrap();
        fixture
    }

    fn write_image(&self, pixel: [u8; 3]) {
        ::image::RgbImage::from_pixel(2, 2, ::image::Rgb(pixel))
            .save(self.0.join("image.png"))
            .unwrap();
    }

    fn lower(&self, source: &str) -> LowerResult {
        let result = lower(source, &self.0.join("main.mos"));
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        result
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn labelled<'a>(result: &'a LowerResult, label: &str) -> &'a Node {
    result
        .document
        .nodes()
        .find(|node| {
            !matches!(node.kind, NodeKind::Reference | NodeKind::PageReference)
                && node.attributes.get("label") == Some(&AttrValue::Str(label.to_owned()))
        })
        .expect("labelled node")
}

fn block_hashes(result: &LowerResult) -> BTreeMap<String, ContentHash> {
    let mut hashes: BTreeMap<_, _> = ["heading", "paragraph", "image", "figure"]
        .map(|label| (label.to_owned(), labelled(result, label).content_hash()))
        .into();
    for node in result
        .document
        .nodes()
        .filter(|node| node.kind == NodeKind::Bibliography)
    {
        let Some(AttrValue::Str(src)) = node.attributes.get("src") else {
            continue;
        };
        hashes.insert(src.clone(), node.content_hash());
    }
    assert!(hashes.values().all(|hash| *hash != ContentHash::default()));
    hashes
}

#[test]
fn blocks_survive_relocation_span_shifts_and_resolution_changes() {
    let original = Fixture::new();
    let relocated = Fixture::new();
    let before = original.lower(SOURCE);
    // Shift every byte offset and allocation ID, and change section, figure,
    // and citation numbering plus the generated bibliography entry list.
    let prefix =
        "= Earlier\n\nEarlier citation [@two].\n\n#figure(\"image.png\", caption: \"Earlier\")\n\n";
    let mut after = relocated.lower(&format!("{prefix}{SOURCE}"));
    assert_ne!(
        labelled(&before, "heading").id,
        labelled(&after, "heading").id
    );
    assert_ne!(
        labelled(&before, "paragraph").span,
        labelled(&after, "paragraph").span
    );
    assert_ne!(
        labelled(&before, "heading").attributes["number"],
        labelled(&after, "heading").attributes["number"]
    );
    assert_eq!(block_hashes(&before), block_hashes(&after));
    let hashes = block_hashes(&after);
    assert!(
        crate::resolve(
            &mut after.document,
            &after.bibliography.entries.keys().cloned().collect()
        )
        .is_empty()
    );
    assert_eq!(
        hashes,
        block_hashes(&after),
        "re-entrant resolution preserves authored hashes"
    );
}

#[test]
fn authored_edits_change_the_affected_block() {
    let fixture = Fixture::new();
    let before = fixture.lower(SOURCE);
    for (old, new, label) in [
        ("= Heading", "== Heading", "heading"),
        ("Heading", "Different heading", "heading"),
        ("*styled*", "**styled**", "paragraph"),
        ("paragraph [@one]", "changed [@one]", "paragraph"),
        ("[@one]", "[@two]", "paragraph"),
        ("20pt", "21pt", "image"),
        ("A picture", "A different picture", "image"),
        ("A caption", "A different caption", "figure"),
        (
            "caption: \"A caption\"",
            "caption: \"A caption\", numbered: false",
            "figure",
        ),
    ] {
        let after = fixture.lower(&SOURCE.replace(old, new));
        assert_ne!(
            labelled(&before, label).content_hash(),
            labelled(&after, label).content_hash(),
            "{old} -> {new}"
        );
        for unaffected in ["heading", "paragraph", "image", "figure"]
            .into_iter()
            .filter(|other| *other != label)
        {
            assert_eq!(
                labelled(&before, unaffected).content_hash(),
                labelled(&after, unaffected).content_hash(),
                "unaffected {unaffected}: {old} -> {new}"
            );
        }
    }
}

#[test]
fn external_byte_changes_propagate_only_to_dependent_blocks() {
    let fixture = Fixture::new();
    let source = format!("{SOURCE}\n#bibliography(\"extra.bib\")\n");
    std::fs::write(fixture.0.join("extra.bib"), "@book{unused, title={Unused}}").unwrap();
    let before = block_hashes(&fixture.lower(&source));
    fixture.write_image([30, 20, 10]);
    let image_changed = block_hashes(&fixture.lower(&source));
    for label in ["image", "figure"] {
        assert_ne!(before[label], image_changed[label]);
    }
    for label in ["heading", "paragraph", "refs.bib", "extra.bib"] {
        assert_eq!(before[label], image_changed[label]);
    }
    std::fs::write(
        fixture.0.join("extra.bib"),
        "@book{unused, title={Changed}}",
    )
    .unwrap();
    let bib_changed = block_hashes(&fixture.lower(&source));
    assert_ne!(image_changed["extra.bib"], bib_changed["extra.bib"]);
    for label in ["heading", "paragraph", "image", "figure", "refs.bib"] {
        assert_eq!(image_changed[label], bib_changed[label]);
    }
}

#[test]
fn missing_empty_malformed_and_unloaded_bibliographies_have_distinct_hashes() {
    let fixture = Fixture::new();
    let file = fixture.0.join("main.mos");
    let source = "#bibliography(\"missing.bib\")\n";
    let mut sink = CollectingSink::new();
    let tree = mos_parse::parse(source, &file, &mut sink).unwrap();
    let hash = |result: LowerResult| {
        result
            .document
            .nodes()
            .find(|node| node.kind == NodeKind::Bibliography)
            .unwrap()
            .content_hash()
    };
    let unloaded = hash(Evaluator::evaluate(&tree));
    let missing = hash(lower(source, &file));
    assert_eq!(missing, hash(lower(source, &file)));
    std::fs::write(fixture.0.join("missing.bib"), "").unwrap();
    let empty = hash(fixture.lower(source));
    std::fs::write(fixture.0.join("missing.bib"), "@book{broken").unwrap();
    let malformed = hash(lower(source, &file));
    std::fs::write(fixture.0.join("missing.bib"), BIB).unwrap();
    let valid = hash(fixture.lower(source));
    let unique: std::collections::BTreeSet<_> = [unloaded, missing, empty, malformed, valid].into();
    assert_eq!(unique.len(), 5);
}

#[test]
fn bare_evaluation_and_resolved_lowering_agree_on_authored_nodes() {
    let fixture = Fixture::new();
    let file = fixture.0.join("main.mos");
    let mut sink = CollectingSink::new();
    let tree = mos_parse::parse(SOURCE, &file, &mut sink).unwrap();
    let bare = Evaluator::evaluate(&tree);
    let resolved = crate::lower_tree(&tree);
    for label in ["heading", "paragraph", "image", "figure"] {
        assert_eq!(
            labelled(&bare, label).content_hash(),
            labelled(&resolved, label).content_hash()
        );
    }
    for node in bare.document.nodes() {
        assert_ne!(node.content_hash(), ContentHash::default());
    }
    // Generated output is outside this input boundary and has no authored hash.
    for node in resolved.document.nodes().skip(bare.document.len()) {
        assert_eq!(node.content_hash(), ContentHash::default());
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_keep_bibliographies_unloaded_and_images_fingerprinted() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let fixture = Fixture::new();
    let loaded = block_hashes(&fixture.lower(SOURCE));
    let mut sink = CollectingSink::new();
    let tree = mos_parse::parse(SOURCE, &fixture.0.join("main.mos"), &mut sink).unwrap();
    let unloaded = block_hashes(&Evaluator::evaluate(&tree))["refs.bib"];
    let dir = fixture.0.join(OsStr::from_bytes(b"caf\xe9"));
    std::fs::create_dir(&dir).unwrap();
    std::fs::copy(fixture.0.join("image.png"), dir.join("image.png")).unwrap();
    std::fs::write(dir.join("refs.bib"), BIB).unwrap();
    let result = lower(SOURCE, &dir.join("main.mos"));
    assert!(!result.bibliography_complete);
    assert!(result.bibliography.entries.is_empty());
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.def().code() == mos_core::codes::MOS0041.code()
            && diagnostic.message().contains("non-UTF-8")
    }));
    assert!(result.external_dependencies.iter().any(|dependency| {
        dependency.path == dir.join("refs.bib") && dependency.fingerprint.is_some()
    }));
    let hashes = block_hashes(&result);
    assert_eq!(hashes["refs.bib"], unloaded);
    assert_ne!(hashes["refs.bib"], loaded["refs.bib"]);
    for label in ["image", "figure"] {
        assert_eq!(hashes[label], loaded[label], "use the real image path");
    }

    // The bibliography remains unloaded when its recorded bytes change, while
    // image changes must still reach the image and its containing figure.
    std::fs::write(dir.join("refs.bib"), "@book{changed, title={Changed}}").unwrap();
    fixture.write_image([30, 20, 10]);
    std::fs::copy(fixture.0.join("image.png"), dir.join("image.png")).unwrap();
    let changed = block_hashes(&lower(SOURCE, &dir.join("main.mos")));
    assert_eq!(changed["refs.bib"], unloaded);
    for label in ["image", "figure"] {
        assert_ne!(
            changed[label], hashes[label],
            "hash the changed image bytes"
        );
    }
}

#[test]
fn decoded_metadata_and_file_timestamps_do_not_enter_authored_hashes() {
    let fixture = Fixture::new();
    let mut sink = CollectingSink::new();
    let tree = mos_parse::parse(SOURCE, &fixture.0.join("main.mos"), &mut sink).unwrap();
    let mut result = Evaluator::evaluate(&tree);
    let before = block_hashes(&result);
    let image_id = labelled(&result, "image").id;
    let attrs = &mut result.document.get_mut(image_id).unwrap().attributes;
    for (key, value) in [
        (
            "resolved_path",
            AttrValue::Str("/another/location/image.png".into()),
        ),
        ("label_span.start", AttrValue::Int(1)),
        ("label_span.end", AttrValue::Int(2)),
        (PIXEL_WIDTH_ATTR, AttrValue::Int(999)),
        (PIXEL_HEIGHT_ATTR, AttrValue::Int(888)),
        (COLOR_SPACE_ATTR, AttrValue::Str("Changed".into())),
        (BITS_PER_COMPONENT_ATTR, AttrValue::Int(16)),
        (PIXELS_ATTR, AttrValue::Bytes(vec![255; 12].into())),
    ] {
        attrs.insert(key.to_owned(), value);
    }
    for dependency in &mut result.external_dependencies {
        if let Some(fingerprint) = &mut dependency.fingerprint {
            fingerprint.modified = Some(std::time::UNIX_EPOCH);
            fingerprint.observed = std::time::UNIX_EPOCH;
            fingerprint.identity = crate::FileIdentity::NONE;
        }
    }
    super::stamp(&mut result.document, &result.external_dependencies);
    assert_eq!(before, block_hashes(&result));
}

#[test]
fn list_and_raw_blocks_include_kind_structure_and_text() {
    let file = PathBuf::from("test.mos");
    let hash = |source: &str, kind| {
        let result = lower(source, &file);
        assert!(!result.has_errors(), "{:?}", result.diagnostics);
        result
            .document
            .nodes()
            .find(|node| node.kind == kind)
            .unwrap()
            .content_hash()
    };
    let list = hash("- one\n- two\n", NodeKind::List);
    assert_ne!(list, hash("- two\n- one\n", NodeKind::List));
    assert_ne!(list, hash("- one\n", NodeKind::List));
    assert_ne!(list, hash("1. one\n2. two\n", NodeKind::List));
    let raw = hash("#pre[[body]]\n", NodeKind::Raw);
    assert_ne!(raw, hash("#pre[[changed]]\n", NodeKind::Raw));
    assert_ne!(raw, hash("#code[[body]]\n", NodeKind::Raw));
}

#[test]
fn attribute_encoding_is_typed_framed_and_float_canonical() {
    assert_eq!(
        super::float_bits(f64::from_bits(0xfff8_0000_0000_0001)),
        0x7ff8_0000_0000_0000
    );
    let hash = |value| {
        let mut hasher = ContentHasher::new();
        super::hash_value(&mut hasher, &value);
        hasher.finish()
    };
    assert_ne!(hash(AttrValue::Int(1)), hash(AttrValue::Str("1".into())));
    assert_ne!(hash(AttrValue::Float(1.0)), hash(AttrValue::Length(1.0)));
    assert_ne!(
        hash(AttrValue::List(vec![
            AttrValue::Str("a".into()),
            AttrValue::Str("bc".into())
        ])),
        hash(AttrValue::List(vec![
            AttrValue::Str("ab".into()),
            AttrValue::Str("c".into())
        ]))
    );
    assert_ne!(
        hash(AttrValue::List(vec![AttrValue::List(vec![])])),
        hash(AttrValue::List(vec![]))
    );
    assert_eq!(hash(AttrValue::Float(-0.0)), hash(AttrValue::Float(0.0)));
    assert_eq!(
        hash(AttrValue::Float(f64::NAN)),
        hash(AttrValue::Float(f64::from_bits(0x7ff8_0000_0000_0001)))
    );
    assert_eq!(
        hash(AttrValue::Length(1.001)),
        hash(AttrValue::Length(1.002))
    );
    assert_ne!(
        hash(AttrValue::Length(1.001)),
        hash(AttrValue::Length(1.02))
    );
}
