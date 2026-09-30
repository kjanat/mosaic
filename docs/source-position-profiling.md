# LSP source-position profiling

## Method

Measured locally on 2026-09-29 with Rust 1.98.0, the workspace's optimized bench profile, and `cargo
bench -p mos-lsp --bench source_positions`. The baseline was merged PR #179 (`e3aaeac`), using the
same generated documents and timed calls before index integration. Each measurement warms up once,
then averages complete iterations over at least 200 ms. Slow baseline cases only fit one or two
iterations; these are local observations, not CI performance thresholds or statistically precise
latency guarantees.

Each document has N same-level headings, blank lines, and paragraphs containing Unicode plus one
unresolved reference per heading. Parsing and lowering happen before timing. Diagnostics measure
`from_result`; symbols measure `document_symbols`. Indexed measurements use their `_indexed`
counterparts with one retained snapshot. Construction is reported separately and includes copying
the source string into shared storage. The benchmark also compares the retained scan-based
conversion helpers with indexed conversion in the same process.

## Results

Times below are milliseconds per complete diagnostic/symbol projection:

| Headings / diagnostics | Source bytes | Diagnostics before | Diagnostics indexed | Symbols before | Symbols indexed | Index construction |
| ---------------------- | ------------ | ------------------ | ------------------- | -------------- | --------------- | ------------------ |
| 100                    | 6,580        | 0.517              | 0.048               | 1.169          | 0.234           | 0.008              |
| 1,000                  | 67,780       | 49.403             | 0.496               | 101.391        | 3.179           | 0.076              |
| 5,000                  | 347,780      | 1,260.479          | 2.570               | 2,531.355      | 21.687          | 0.389              |

The small-document indexed row is from the first successful indexed run; the larger rows are from
the final run. Repeated runs varied, especially symbol allocation time, without changing which
operation dominated the baseline.

## Cause

The old byte-to-position helper walks every byte before the requested offset to count newlines, then
counts UTF-16 units on that line. Its reverse helper walks from the document start to locate the
requested line. Repeating these operations throughout a growing document produces quadratic
aggregate scanning.

On the 5,000-heading fixture, a batch of 5,000 forward conversions took about 627 ms, compared with
0.170 ms indexed. Reverse conversions took 468 ms versus 0.040 ms indexed. Diagnostic ranges need
two forward conversions each; symbol ranges plus selection ranges need four. Thus the measured scan
cost predicts approximately 1.25 s for diagnostics and 2.51 s for symbols, closely matching the
measured baseline projection times. This isolates repeated position scans as the principal cause for
these workloads.

A separate 524,288-byte single Unicode line, sampled at 384 positions, took 110.598 ms with the scan
helper versus 0.031 ms indexed. Index construction took 0.319 ms. Sparse checkpoints are needed
here: storing only line starts would still repeatedly scan the entire line prefix.

## Implementation and limits

`mos_core::LineIndex` owns immutable UTF-8 text, line starts, and UTF-16 checkpoints roughly every
256 bytes within long lines. Queries binary-search the relevant index entries and scan a bounded
suffix. Short lines need no checkpoints. On 64-bit targets, index entries use 16 bytes per line and
16 bytes per checkpoint, plus shared allocation metadata and the source text. Cloning shares all
storage. Keeping text with the index prevents mismatching revisions.

The LSP retains one index per open Mosaic source revision. Resource snapshots lazily retain an index
per requested valid UTF-8 resource, used by both diagnostics and citation definitions; this text
copy and its index are allocated only on demand. No filesystem reread is introduced. Existing
single-position string helpers remain allocation-free scans for occasional calls.

These gains concern source-position conversion and projection, not parsing, lowering, JSON-RPC,
filesystem freshness checks, or whole-editor response time. Symbol generation still walks nodes and
allocates JSON values. Further changes to those stages need their own measurements.
