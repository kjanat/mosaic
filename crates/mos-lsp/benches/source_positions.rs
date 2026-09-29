//! Run with `cargo bench -p mos-lsp --bench source_positions`.
#![allow(
    clippy::print_stdout,
    reason = "benchmark reports measurements to stdout"
)]
use std::hint::black_box;
use std::path::Path;
use std::time::{Duration, Instant};

fn measure(name: &str, mut operation: impl FnMut()) {
    operation();
    let start = Instant::now();
    let mut iterations = 0_u32;
    while start.elapsed() < Duration::from_millis(200) {
        operation();
        iterations += 1;
    }
    println!(
        "{name}: {:.3} us/iteration ({iterations} iterations)",
        start.elapsed().as_secs_f64() * 1e6 / f64::from(iterations)
    );
}

fn main() {
    for count in [100, 1_000, 5_000] {
        let source: String = (0..count)
            .map(|index| {
                format!(
                    "= Heading {index} 😀\n\nSee @missing{index}. A paragraph with Unicode 字.\n\n"
                )
            })
            .collect();
        let path = Path::new("/virtual/profile.mos");
        let lowered = mos_eval::lower(&source, path);
        println!(
            "\n{count} headings, {} bytes, {} diagnostics",
            source.len(),
            lowered.diagnostics.len()
        );
        let index = mos_core::LineIndex::new(source.as_str());
        measure("index construction", || {
            black_box(mos_core::LineIndex::new(black_box(source.as_str())));
        });
        measure("diagnostics", || {
            black_box(mos_lsp::diagnostics::from_result_indexed(
                path,
                black_box(&index),
                &lowered,
            ));
        });
        measure("symbols", || {
            black_box(mos_lsp::document_symbol::document_symbols_indexed(
                &lowered.document,
                black_box(&index),
            ));
        });
        let offsets: Vec<_> = source
            .match_indices('@')
            .map(|(offset, _)| offset)
            .collect();
        measure("byte to UTF-16", || {
            for offset in &offsets {
                black_box(mos_lsp::byte_to_position(black_box(&source), *offset));
            }
        });
        let positions: Vec<_> = offsets
            .iter()
            .map(|offset| mos_lsp::byte_to_position(&source, *offset))
            .collect();
        measure("UTF-16 to byte", || {
            for position in &positions {
                black_box(mos_lsp::position_to_byte(black_box(&source), *position));
            }
        });
        measure("indexed byte to UTF-16", || {
            for offset in &offsets {
                black_box(index.utf16_position(black_box(*offset)));
            }
        });
        measure("indexed UTF-16 to byte", || {
            for position in &positions {
                black_box(index.byte_offset(
                    black_box(position.line as usize),
                    position.character as usize,
                ));
            }
        });
    }
    let source = "a😀字".repeat(65_536);
    let index = mos_core::LineIndex::new(source.as_str());
    let offsets: Vec<_> = source
        .char_indices()
        .step_by(512)
        .map(|(offset, _)| offset)
        .collect();
    println!(
        "\nlong Unicode line: {} bytes, {} lookups",
        source.len(),
        offsets.len()
    );
    measure("long-line index construction", || {
        black_box(mos_core::LineIndex::new(black_box(source.as_str())));
    });
    measure("long-line scan", || {
        for offset in &offsets {
            black_box(mos_lsp::byte_to_position(black_box(&source), *offset));
        }
    });
    measure("long-line indexed", || {
        for offset in &offsets {
            black_box(index.utf16_position(black_box(*offset)));
        }
    });
}
