# Compiler and LSP architecture direction

Status: exploration note for issue #130. This records recommendations and gates, not shipped
behavior or approval for a parser/LSP rewrite.

## Decision

Mosaic should borrow **boundaries** from rust-analyzer without adopting its complete implementation
stack. Keep the direct compiler pipeline and thin synchronous LSP while the project is small. Make
filesystem inputs explicit, put a compiler-owned analysis API between protocol adapters and semantic
operations, and add incremental machinery only after measurements show that coarse recomputation is
a problem.

The intended direction is:

```text
source/resource inputs -> syntax -> semantic document -> diagnostics/layout
                                      |
                                      v
                              analysis operations
                               /              \
                         CLI adapter       LSP adapter
```

This is dependency direction, not necessarily a crate-per-box plan. In particular, an `AnalysisHost`
crate or a query database should not be created until at least two consumers need the same operation
and its input ownership is understood.

## Corrections to the original research report

The June 2026 report is directionally sound, but several proposed first steps have already landed or
need narrowing against the current code:

- The workspace MSRV is already Rust 1.96. Adopting `assert_matches!` is cleanup, not an MSRV
  project, and should be done only where it improves a test over the existing assertion.
- `LowerResult` now records every image and bibliography read as an `ExternalDependency` with a
  fingerprint. The LSP cache does cache these lowerings and validates every dependency before reuse;
  it no longer needs the proposed boolean-only impurity model.
- `mos-cache` already defines the first typed dependency identities, and
  `docs/incremental-dependencies.md` defines current and deferred hash boundaries. Future work
  should extend that design rather than introduce a competing resource/dependency vocabulary.
- `NodeId` provides arena identity but is monotonic within a `Document`, not stable across
  lowerings. It must not be described as a cache-stable syntax or semantic identity.
- `SourceSpan` owns a `PathBuf` and `usize` offsets. Merely switching the offsets to a range type
  cannot make the span cheap or `Copy`; file identity and text range would first need to be split.

## Keep, change, or defer

| Candidate                     | Decision                       | Revisit when                                                                                 |
| ----------------------------- | ------------------------------ | -------------------------------------------------------------------------------------------- |
| Current hand-written parser   | **Keep**                       | Recovery or tooling requirements cannot be met without lossless syntax.                      |
| rowan                         | **Spike only**                 | The formatter or a structural refactor needs trivia-preserving edits.                        |
| Current semantic `Document`   | **Keep** as the HIR-like layer | Syntax concerns leak into consumers, or a distinct typed HIR solves a demonstrated problem.  |
| salsa                         | **Defer**                      | Inputs are explicit and profiling shows repeated analysis/invalidation cost.                 |
| Analysis facade               | **Change incrementally**       | Extract one compiler-owned operation when CLI and LSP genuinely duplicate it.                |
| Current stdio LSP loop        | **Keep**                       | Cancellation, concurrency, workspace indexing, progress, or custom methods become necessary. |
| tower-lsp-server + tokio      | **Defer**                      | One of those async requirements lands with an end-to-end test.                               |
| Core diagnostics              | **Keep**                       | Always remain the source for codes, spans, annotations, and suggestions.                     |
| miette                        | **Optional CLI adapter**       | A concrete diagnostic UX issue justifies the dependency and snapshot coverage.               |
| tracing                       | **Adopt narrowly**             | There is a consumer for the events and an explicit stderr/logging policy for LSP.            |
| insta                         | **Adopt selectively**          | A stable, reviewed representation is less brittle than focused assertions.                   |
| proptest                      | **Adopt for invariants**       | Start with parser recovery and span safety; retain a failing seed as a regression test.      |
| Built-in pass registry        | **Defer**                      | At least two configurable passes need shared ordering/configuration.                         |
| inventory/static registration | **Defer**                      | Registration boilerplate is a measured problem.                                              |
| Dynamic Rust plugins          | **Reject for now**             | Requires a separately designed stable process/Wasm boundary, not Rust ABI loading.           |

## Recommended sequence

### 1. Characterize before abstracting

Add benchmarks or lightweight counters around the operations suspected of being expensive: parse,
lower/resolve, layout, and common LSP requests. Record representative document sizes and warm/cold
behavior. Instrumentation should have near-zero cost when disabled and must never write protocol
noise to LSP stdout.

This evidence decides whether the next investment belongs in parser recovery, an analysis API, the
existing cache, or layout. It avoids introducing salsa to optimize work that is not dominant.

### 2. Make resource access injectable

The most valuable architectural slice is smaller than a general `AnalysisHost`: introduce a
compiler-side resource-reader boundary for image and bibliography bytes while preserving the
existing path resolution and `ExternalDependency` result. Production uses the filesystem; tests use
an in-memory reader. The result should still report exactly which resolved resources were observed
and their fingerprints.

This enables deterministic tests and future query inputs without committing to salsa. Do not expose
LSP URI types, JSON values, or async traits through this boundary.

### 3. Extract analysis operations one at a time

When CLI and LSP share a real semantic operation, expose it from a compiler-owned analysis module
using compiler types. A useful first API is likely an immutable source snapshot plus lowered result
and a line index, with operations such as diagnostics or definition lookup layered over it.

Keep ownership rules explicit:

- the outer host owns source text and resource snapshots;
- compiler analysis performs no hidden protocol I/O;
- diagnostics and suggestions remain `mos-core` values;
- the LSP layer only converts positions and wire types;
- cancellation, if later needed, is an input checked at coarse boundaries rather than an async type
  threaded through every compiler crate.

Do not create a facade that merely renames `mos_eval::lower` or centralizes unrelated helpers.

### 4. Add focused test tools

Use property tests for contracts that examples undersample:

1. parsing arbitrary UTF-8 never panics;
2. emitted spans are ordered, in bounds, and on UTF-8 boundaries where slicing occurs;
3. diagnostics are deterministic for identical source and resource inputs;
4. parser recovery always makes progress.

Use snapshots for intentionally holistic outputs such as a recovered syntax tree plus diagnostics or
a complete LSP response. Prefer ordinary assertions for individual codes, spans, and suggestions;
large snapshots can hide semantically important changes in review.

### 5. Run bounded spikes, then delete or graduate them

A rowan spike should cover one awkward, trivia-sensitive slice rather than only the easiest grammar:
headings and paragraphs with comments, references, malformed inline delimiters, and recovery.
Compare source fidelity, diagnostic spans, typed-wrapper ergonomics, edit behavior, and
allocation/time against the existing parser. Keep the spike out of the production path unless it
wins against written criteria.

A `FileId + TextRange` spike should measure size and clone reduction and define overflow behavior.
Keep the public `SourceSpan` API compatible until all path-bearing diagnostic and source-map uses
are accounted for. Prefer the same range representation as rowan only if rowan is actually selected;
do not add `text-size` solely for aesthetic consistency.

## Gates for larger dependencies

### salsa gate

Adopt salsa only when all of the following are true:

- source and external resource contents are explicit inputs;
- query outputs are deterministic from those inputs;
- dependency ownership and invalidation rules are documented;
- stable identities exist for the intended query keys;
- a benchmark demonstrates that the current cache/recompute strategy misses a target;
- cancellation and accumulator semantics have been prototyped for diagnostics.

Start with parse or a similarly pure query. Do not put layout or filesystem-reading lowering into
the first database slice.

### async LSP gate

Adopt tower-lsp-server/tokio only with a feature that needs cancellation or concurrency and an E2E
test demonstrating it. Before migration, specify shutdown behavior, request cancellation, panic
isolation, logging destinations, and ordering of diagnostics after rapid edits. Framework adoption
alone is not a feature.

### pass-registry gate

First define a pass contract: inputs, outputs, diagnostic ownership, ordering, configuration,
determinism, and whether mutation is allowed. Introduce a built-in registry only when multiple real
passes exercise that contract. Static auto-registration and third-party loading remain separate
questions; neither should shape the initial trait.

## Suggested follow-up issues

Prefer a short dependency chain rather than opening all research bullets as parallel implementation
work:

1. **test: add parser recovery property invariants** — bounded, immediately useful, and independent
   of architecture selection.
2. **design/spike: inject image and bibliography resource reads** — preserve dependency reporting;
   prove production and in-memory readers with tests.
3. **perf: establish compiler and LSP analysis baselines** — define the threshold that would justify
   finer-grained invalidation.
4. **design: extract one shared compiler analysis operation** — only after identifying actual
   CLI/LSP duplication.
5. **explore: compare rowan on a trivia-and-recovery fixture** — time-boxed, with written
   keep/delete criteria.
6. **explore: split file identity from text range** — coordinate with the rowan result and existing
   incremental dependency IDs.

Do not open salsa, async LSP, plugin, or broad parser-rewrite implementation issues until their
gates above are met.
