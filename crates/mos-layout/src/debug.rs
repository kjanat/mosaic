//! Optional geometry and source tracing for layout inspection.
//!
//! Coordinates are PDF points measured rightward/downward from the page top
//! left. Text rectangles use shaped advances and font ascent/descent, rather
//! than glyph ink outlines. Container rectangles enclose emitted content on
//! each page; they do not include trailing paragraph space or blank raw lines.

use std::collections::BTreeMap;

use mos_core::{AttrValue, Node, NodeKind, display_path};
use serde::Serialize;

use crate::{ImagePlacement, Page as LayoutPage, PageStyle, TextRun, ascent, descent};

/// Versioned, deterministic geometry report from an explicitly traced layout.
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub units: &'static str,
    pub origin: &'static str,
    pub pages: Vec<Page>,
}

/// Final page geometry, source blocks, text lines, and image placements.
#[derive(Clone, Debug, Serialize)]
pub struct Page {
    pub number: u32,
    pub bounds: Rect,
    pub content_bounds: Rect,
    pub blocks: Vec<Block>,
    pub lines: Vec<Line>,
    pub images: Vec<Image>,
}

/// A rectangle measured in points from the page's top-left corner.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Rect {
    pub x_pt: f32,
    pub y_pt: f32,
    pub width_pt: f32,
    pub height_pt: f32,
}

impl Rect {
    fn union(self, other: Self) -> Self {
        let x = self.x_pt.min(other.x_pt);
        let y = self.y_pt.min(other.y_pt);
        Self {
            x_pt: x,
            y_pt: y,
            width_pt: (self.x_pt + self.width_pt).max(other.x_pt + other.width_pt) - x,
            height_pt: (self.y_pt + self.height_pt).max(other.y_pt + other.height_pt) - y,
        }
    }
}

/// The semantic block responsible for a placement. Offsets are UTF-8 bytes.
/// File names are display strings; non-UTF-8 paths use replacement characters.
#[derive(Clone, Debug, Serialize)]
pub struct Source {
    pub node_id: u64,
    pub kind: &'static str,
    pub file: String,
    pub byte_start: usize,
    pub byte_end: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl Source {
    fn from_node(node: &Node) -> Self {
        Self {
            node_id: node.id.0,
            kind: match node.kind {
                NodeKind::Section => "section",
                NodeKind::Paragraph => "paragraph",
                NodeKind::Image => "image",
                NodeKind::Figure => "figure",
                NodeKind::List => "list",
                NodeKind::ListItem => "list_item",
                NodeKind::Bibliography => "bibliography",
                NodeKind::Raw => "raw",
                _ => "other",
            },
            file: display_path(&node.span.file),
            byte_start: node.span.start(),
            byte_end: node.span.end(),
            label: match node.attributes.get("label") {
                Some(AttrValue::Str(label)) => Some(label.clone()),
                _ => None,
            },
        }
    }
}

/// Per-page union of the content emitted by a source block and its children.
#[derive(Clone, Debug, Serialize)]
pub struct Block {
    pub source: Source,
    pub bounds: Rect,
}

/// One committed line, including list/bibliography markers and font fallback.
#[derive(Clone, Debug, Serialize)]
pub struct Line {
    pub source: Option<Source>,
    pub bounds: Rect,
    pub baseline_from_top_pt: f32,
    pub runs: Vec<Run>,
}

/// One final text run, indexed into its page's run array.
#[derive(Clone, Debug, Serialize)]
pub struct Run {
    pub run_index: usize,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_text: Option<String>,
    pub font: String,
    pub size_pt: f32,
    pub bounds: Rect,
}

/// One final image placement, without decoded pixel bytes or asset paths.
#[derive(Clone, Debug, Serialize)]
pub struct Image {
    pub source: Option<Source>,
    pub image_index: usize,
    pub image_id: u32,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub bounds: Rect,
}

#[derive(Default)]
struct PageTrace {
    blocks: BTreeMap<u64, Block>,
    lines: Vec<Line>,
    images: Vec<Image>,
}

#[derive(Default)]
pub(super) struct Recorder {
    active: Vec<Source>,
    pages: BTreeMap<u32, PageTrace>,
    pending_runs: Vec<Run>,
}

impl Recorder {
    pub(super) fn begin_block(&mut self, node: &Node) {
        self.active.push(Source::from_node(node));
    }

    pub(super) fn end_block(&mut self) {
        self.active.pop();
    }

    pub(super) fn run(&mut self, run_index: usize, run: &TextRun, advance_pt: f32) {
        let ascender = ascent(run.font, run.size_pt);
        self.pending_runs.push(Run {
            run_index,
            text: run.text.clone(),
            actual_text: run.actual_text.clone(),
            font: run.font.pdf_base_name().to_owned(),
            size_pt: run.size_pt,
            bounds: Rect {
                x_pt: run.x_pt,
                y_pt: run.baseline_from_top_pt - ascender,
                width_pt: advance_pt,
                height_pt: ascender + descent(run.font, run.size_pt),
            },
        });
    }

    pub(super) fn line(&mut self, page: u32, baseline_from_top_pt: f32) {
        let runs = std::mem::take(&mut self.pending_runs);
        let Some(bounds) = runs.iter().map(|run| run.bounds).reduce(Rect::union) else {
            return;
        };
        self.extend_blocks(page, bounds);
        self.pages.entry(page).or_default().lines.push(Line {
            source: self.active.last().cloned(),
            bounds,
            baseline_from_top_pt,
            runs,
        });
    }

    pub(super) fn image(&mut self, page: u32, image_index: usize, image: &ImagePlacement) {
        let bounds = Rect {
            x_pt: image.x_pt,
            y_pt: image.top_from_top_pt,
            width_pt: image.width_pt,
            height_pt: image.height_pt,
        };
        self.extend_blocks(page, bounds);
        self.pages.entry(page).or_default().images.push(Image {
            source: self.active.last().cloned(),
            image_index,
            image_id: image.handle.id,
            pixel_width: image.handle.pixel_width,
            pixel_height: image.handle.pixel_height,
            bounds,
        });
    }

    fn extend_blocks(&mut self, page: u32, bounds: Rect) {
        let blocks = &mut self.pages.entry(page).or_default().blocks;
        for source in &self.active {
            blocks
                .entry(source.node_id)
                .and_modify(|block| {
                    block.bounds = block.bounds.union(bounds);
                })
                .or_insert_with(|| Block {
                    source: source.clone(),
                    bounds,
                });
        }
    }

    pub(super) fn finish(mut self, pages: &[LayoutPage], style: PageStyle) -> Report {
        Report {
            schema_version: 1,
            units: "pt",
            origin: "top-left",
            pages: pages
                .iter()
                .map(|page| {
                    let trace = self.pages.remove(&page.number).unwrap_or_default();
                    Page {
                        number: page.number,
                        bounds: Rect {
                            x_pt: 0.0,
                            y_pt: 0.0,
                            width_pt: page.width_pt,
                            height_pt: page.height_pt,
                        },
                        content_bounds: Rect {
                            x_pt: style.margin,
                            y_pt: style.margin,
                            width_pt: page.width_pt - 2.0 * style.margin,
                            height_pt: page.height_pt - 2.0 * style.margin,
                        },
                        blocks: trace.blocks.into_values().collect(),
                        lines: trace.lines,
                        images: trace.images,
                    }
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests;
