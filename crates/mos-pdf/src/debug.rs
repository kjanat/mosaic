//! Vector overlays for the geometry recorded by layout. No layout policy.

use mos_core::{CoreError, Diagnostic, Result, codes};
use mos_layout::{
    PageGraph,
    debug::{Page, Rect, Report},
};
use pdf_writer::{Content, Name, Str};

pub(crate) const LEGEND_HEIGHT_PT: f32 = 36.0;
pub(crate) const FONT_NAME: Name<'_> = Name(b"LayoutDebug");

pub(crate) fn canvas_width(width: f32) -> f32 {
    width.max(420.0)
}

// Colors and stroke patterns are shared by the geometry and its legend.
struct Style {
    rgb: [f32; 3],
    width: f32,
    dash: &'static [f32],
}

const CONTENT: Style = Style {
    rgb: [0.4, 0.4, 0.4],
    width: 0.6,
    dash: &[4.0, 3.0],
};
const BLOCK: Style = Style {
    rgb: [0.0, 0.3, 0.85],
    width: 0.9,
    dash: &[3.0, 2.0],
};
const LINE: Style = Style {
    rgb: [0.0, 0.55, 0.3],
    width: 0.4,
    dash: &[],
};
const RUN: Style = Style {
    rgb: [0.65, 0.2, 0.7],
    width: 0.4,
    dash: &[1.0, 1.5],
};
const IMAGE: Style = Style {
    rgb: [0.9, 0.4, 0.0],
    width: 1.0,
    dash: &[],
};
const BASELINE: Style = Style {
    rgb: [0.9, 0.1, 0.15],
    width: 0.6,
    dash: &[],
};

pub(crate) fn validate(graph: &PageGraph, report: &Report) -> Result<()> {
    if report.schema_version != 1
        || report.units != "pt"
        || report.origin != "top-left"
        || graph.pages.len() != report.pages.len()
        || graph.pages.iter().zip(&report.pages).any(|(page, trace)| {
            page.number != trace.number
                || trace.bounds
                    != (Rect {
                        x_pt: 0.0,
                        y_pt: 0.0,
                        width_pt: page.width_pt,
                        height_pt: page.height_pt,
                    })
        })
    {
        return Err(CoreError::Diagnostic(Box::new(Diagnostic::simple(
            &codes::MOS0051,
            None,
            "debug layout report must come from the same layout result as the page graph",
        ))));
    }
    Ok(())
}

impl Style {
    fn apply(&self, content: &mut Content) {
        let [r, g, b] = self.rgb;
        content
            .set_stroke_rgb(r, g, b)
            .set_line_width(self.width)
            .set_dash_pattern(self.dash.iter().copied(), 0.0);
    }
}

fn rectangle(content: &mut Content, height: f32, bounds: Rect, style: &Style) {
    style.apply(content);
    content
        .rect(
            bounds.x_pt,
            height - bounds.y_pt - bounds.height_pt,
            bounds.width_pt,
            bounds.height_pt,
        )
        .stroke();
}

pub(crate) fn overlay(page: &Page) -> Vec<u8> {
    let height = page.bounds.height_pt;
    let mut content = Content::new();
    content.save_state();
    // Keep the legend outside the original paper even for zero-margin pages.
    content
        .set_fill_rgb(0.96, 0.97, 0.99)
        .rect(
            0.0,
            height,
            canvas_width(page.bounds.width_pt),
            LEGEND_HEIGHT_PT,
        )
        .fill_nonzero();
    content
        .set_stroke_rgb(0.5, 0.5, 0.5)
        .set_line_width(0.5)
        .rect(0.25, 0.25, page.bounds.width_pt - 0.5, height - 0.5)
        .stroke();
    legend(&mut content, page);

    rectangle(&mut content, height, page.content_bounds, &CONTENT);
    for block in &page.blocks {
        rectangle(&mut content, height, block.bounds, &BLOCK);
    }
    for line in &page.lines {
        rectangle(&mut content, height, line.bounds, &LINE);
        for run in &line.runs {
            rectangle(&mut content, height, run.bounds, &RUN);
        }
        BASELINE.apply(&mut content);
        let y = height - line.baseline_from_top_pt;
        content
            .move_to(line.bounds.x_pt, y)
            .line_to(line.bounds.x_pt + line.bounds.width_pt, y)
            .stroke();
    }
    for image in &page.images {
        rectangle(&mut content, height, image.bounds, &IMAGE);
    }
    content.restore_state();
    content.finish().to_vec()
}

fn legend(content: &mut Content, page: &Page) {
    let font = FONT_NAME;
    content
        .set_fill_rgb(0.15, 0.2, 0.3)
        .begin_text()
        .set_font(font, 8.0)
        .set_text_matrix([1.0, 0.0, 0.0, 1.0, 12.0, page.bounds.height_pt + 23.0])
        .show(Str(format!(
            "LAYOUT DEBUG | page {} | units: pt",
            page.number
        )
        .as_bytes()))
        .end_text();
    let mut x = 12.0;
    for (label, style) in [
        ("content", &CONTENT),
        ("block", &BLOCK),
        ("line", &LINE),
        ("run", &RUN),
        ("image", &IMAGE),
        ("baseline", &BASELINE),
    ] {
        style.apply(content);
        let y = page.bounds.height_pt + 10.0;
        content
            .move_to(x, y + 2.0)
            .line_to(x + 12.0, y + 2.0)
            .stroke();
        content
            .begin_text()
            .set_font(font, 7.0)
            .set_text_matrix([1.0, 0.0, 0.0, 1.0, x + 16.0, y])
            .show(Str(label.as_bytes()))
            .end_text();
        x += 64.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_incompatible_reports() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let layout = mos_layout::LayoutEngine::new()
            .layout_with_debug(&mos_core::Document::new("debug.mos".into()));
        let report = layout.debug.ok_or("tracing requested")?;
        validate(&layout.graph, &report)?;
        let mut incompatible = [report.clone(), report.clone(), report.clone(), report];
        incompatible[0].pages.clear();
        incompatible[1].pages[0].number += 1;
        incompatible[2].pages[0].bounds.height_pt += 1.0;
        incompatible[3].schema_version += 1;
        for report in incompatible {
            if validate(&layout.graph, &report).is_ok() {
                return Err("incompatible report accepted".into());
            }
        }
        Ok(())
    }
}
