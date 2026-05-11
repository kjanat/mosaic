# Mosaic: a modern constraint-based typesetting engine

> Let’s design a new typesetting engine from scratch.
>
> Call it **Mosaic** for now, because apparently every serious system needs a name that sounds like
> either a browser from 1993 or a startup that sells calendars to developers.

## Core idea

Mosaic is not a LaTeX clone.

It is a **document compiler** that turns a semantic document graph into one or more outputs:

```text
source files
  ↓
semantic document graph
  ↓
resolved reference graph
  ↓
layout constraint graph
  ↓
paginated output graph
  ↓
PDF / HTML / EPUB / SVG / print package
```

The key principle:

> Documents are graphs, not token streams.

LaTeX mostly behaves like a sequential program that emits boxes. Mosaic should behave like a
compiler plus incremental layout solver.

---

# 1. Implementation language

## Use Rust for the engine

Rust is the right default for the core engine.

Reasons:

```text
- memory safety
- high performance
- good CLI tooling
- good package/dependency ecosystem
- excellent parsing libraries
- strong type system
- good WASM support
- easy embedding in editors
- good cross-platform builds
```

The engine should be written as a set of Rust crates:

```text
mosaic-cli        command-line interface
mosaic-core       document model, IDs, diagnostics
mosaic-parse      parser for the source language
mosaic-eval       expression and scripting evaluator
mosaic-layout     layout engine
mosaic-pdf        PDF backend
mosaic-html       HTML backend
mosaic-fonts      font discovery, shaping, metrics
mosaic-bib        bibliography/citation engine
mosaic-cache      incremental build cache
mosaic-lsp        language server
mosaic-packages   package manager integration
```

No giant monolith. No “everything is global mutable state.” We have suffered enough.

---

# 2. Source language

The source language should be declarative first, programmable second.

Call the file extension:

```text
.mos
```

A document might look like this:

```mos
#set document(
  title: "Pulmonary Embolism in Primary Care",
  author: "Kaj Kowalski",
  date: today(),
  language: "en"
)

#set page(
  paper: "A4",
  margin: 24mm,
  numbering: "bottom-center"
)

#set text(
  font: "Libertinus Serif",
  size: 11pt,
  leading: 1.35
)

= Introduction

Pulmonary embolism is a clinically heterogeneous condition @konstantinides2020.

As shown in @fig:wells, structured pretest probability assessment improves diagnostic reasoning.

#figure(
  image("wells-score.pdf"),
  caption: "Simplified Wells score decision flow.",
  placement: near,
  priority: high
) <fig:wells>

== Diagnostic approach

The D-dimer threshold may be adjusted according to age:

$ threshold = age times 10 " µg/L" $
```

Important design choice: this is **not TeX syntax**.

TeX syntax is powerful, but it is also what happens when a printer demon gets tenure.

---

# 3. Language design

## 3.1 Markup mode

Basic structure should be lightweight:

```mos
= Section
== Subsection
=== Subsubsection

Regular paragraph text with *emphasis*, **strong text**, `inline code`, and @citation.

See @fig:scan and @eq:bayes.
```

## 3.2 Function calls

All nontrivial document constructs use explicit function calls:

```mos
#figure(
  image("ctpa.png"),
  caption: "CT pulmonary angiography showing segmental embolus.",
  placement: top,
) <fig:ctpa>
```

## 3.3 Labels

Labels are attached to semantic objects:

```mos
= Methods <sec:methods>

#equation[
  P(A|B) = frac(P(B|A) P(A), P(B))
] <eq:bayes>
```

References are semantic:

```mos
See @sec:methods.
See @eq:bayes.
```

The engine knows whether `@eq:bayes` is an equation, figure, section, theorem, table, or whatever
else humans have decided needs numbering.

## 3.4 Math syntax

Use a clean math mode inspired by Typst, AsciiMath, and LaTeX, but not blindly compatible.

Example:

```mos
$ integral_0^infinity e^(-x^2) dif x = sqrt(pi) / 2 $
```

Displayed equation:

```mos
#equation[
  RR = frac(a / (a + b), c / (c + d))
] <eq:relative-risk>
```

Support aliases for common LaTeX commands for migration:

```mos
$ \alpha + \beta = \gamma $
```

But internally normalize to Mosaic’s math AST.

That part matters. Math should become structured data, not opaque glyph soup.

---

# 4. No arbitrary macro hell

Mosaic should not have TeX-style macro expansion as the foundation.

Instead, it should have:

```text
- typed functions
- structured values
- immutable document nodes
- explicit side effects only where allowed
- sandboxed scripting
```

A package can define:

```mos
#let warning_box(title, body) = block(
  border: 1pt solid red,
  padding: 8pt,
  radius: 4pt,
)[
  **#title**

  #body
]
```

But it cannot silently mutate global numbering, redefine paragraph parsing, or turn `\section` into
a database client because some academic in 2004 had a vision.

Global style modifications must be explicit:

```mos
#set heading(level: 1, numbering: "1.")
#set figure(numbering: "Figure 1")
#set citation(style: "vancouver")
```

---

# 5. Internal document model

Everything becomes nodes.

## 5.1 Core node types

```rust
enum NodeKind {
    Document,
    Section,
    Paragraph,
    Text,
    Emphasis,
    Strong,
    Math,
    Equation,
    Figure,
    Table,
    Citation,
    Reference,
    Theorem,
    Footnote,
    Bibliography,
    Raw,
}
```

Each node has:

```rust
struct Node {
    id: NodeId,
    kind: NodeKind,
    span: SourceSpan,
    content_hash: ContentHash,
    style_id: StyleId,
    children: Vec<NodeId>,
    attributes: AttrMap,
}
```

Node IDs must be stable across builds when possible.

Not:

```text
node 472 because it happened to be parsed 472nd
```

Better:

```text
hash(file path + syntactic position + explicit label + local structure)
```

Stable IDs are essential for incremental builds.

---

# 6. Compilation pipeline

## Stage 1: Parse

Input:

```text
.mos files
```

Output:

```text
Concrete syntax tree
```

The parser should preserve:

```text
- source spans
- comments
- formatting where useful
- recoverable errors
```

This enables editor tooling, autoformatting, and good diagnostics.

## Stage 2: Lower to semantic graph

Turn syntax into typed document nodes.

Example:

```mos
= Methods <sec:methods>
```

becomes:

```text
SectionNode {
  level: 1,
  title: "Methods",
  label: "sec:methods"
}
```

## Stage 3: Resolve semantic structure

Resolve:

```text
- section hierarchy
- counters
- theorem environments
- figure/table numbering
- equation numbering
- labels
- citations
- document metadata
```

This stage should not do page layout yet.

Semantic numbering should be mostly independent of pagination.

## Stage 4: Resolve external data

Load:

```text
- bibliography databases
- images
- fonts
- package dependencies
- included files
- data files
```

All external data becomes tracked dependencies.

If `references.bib` changes, the engine knows what depends on it. Revolutionary stuff, meaning
normal software engineering.

## Stage 5: Inline layout

Resolve:

```text
- font shaping
- glyph runs
- line breaking
- inline math
- inline references
- citation clusters
```

This produces layout fragments with measurable dimensions.

For example:

```rust
struct InlineBox {
    width: Abs,
    height: Abs,
    depth: Abs,
    glyph_runs: Vec<GlyphRun>,
    dependencies: Vec<NodeId>,
}
```

Use HarfBuzz or equivalent shaping. Do not invent font shaping unless the goal is lifelong
suffering.

## Stage 6: Block layout

Turn paragraphs, figures, equations, lists, tables, and code blocks into block boxes.

```rust
enum Block {
    Paragraph(ParagraphBox),
    DisplayMath(MathBox),
    Figure(FigureBox),
    Table(TableBox),
    Heading(HeadingBox),
    Footnote(FootnoteBox),
}
```

## Stage 7: Pagination and float solving

This is where the real beast lives.

The page solver receives:

```text
- ordered block stream
- float constraints
- footnote constraints
- page style rules
- widow/orphan rules
- keep-with-next rules
- region constraints
```

It outputs:

```text
PageGraph
```

With page assignments:

```rust
struct Page {
    number: PageNumber,
    regions: Vec<PageRegion>,
    placed_blocks: Vec<PlacedBlock>,
    floats: Vec<PlacedFloat>,
    footnotes: Vec<PlacedFootnote>,
}
```

## Stage 8: Global stabilization

Resolve things that depend on final pagination:

```text
- table of contents page numbers
- list of figures
- list of tables
- page references
- index
- cross-reference page text
```

If these change layout, run targeted invalidation.

Not full rebuild. Not external ritual. Internal fixpoint.

## Stage 9: Emit output

Backends:

```text
PDF
HTML
EPUB
SVG pages
debug layout view
```

PDF is the primary target. HTML should be semantic, not just a pile of absolutely positioned
rectangles.

---

# 7. Dependency graph

This is the heart.

Every computed artifact declares dependencies.

```rust
// `DepId` and `DepKind` are deferred to MVP 5 (incremental builds);
// the MVP 0 scaffold tracks dependencies inline on `Node` instead.
struct DepNode {
    id: DepId,
    kind: DepKind,
    inputs: Vec<DepId>,
    output_hash: ContentHash,
}
```

Examples:

```text
Paragraph layout
  depends on:
    paragraph text
    font metrics
    page width
    style

Figure box
  depends on:
    image file
    caption node
    figure style
    available width

Reference text
  depends on:
    target label
    target number
    maybe target page

TOC entry
  depends on:
    heading text
    heading number
    heading page
```

When something changes, mark dependents dirty:

```text
changed: figure caption
dirty:
  figure box
  list of figures entry
  references to figure if caption affects page position
  local pagination region
```

Clean nodes are reused from cache.

---

# 8. Incremental build model

The build process should feel like this:

```bash
mos build paper.mos
```

For live preview:

```bash
mos watch paper.mos
```

For diagnostics:

```bash
mos check paper.mos
```

For dependency inspection:

```bash
mos graph paper.mos
```

For performance analysis:

```bash
mos profile paper.mos
```

Example output:

```text
Parsed 37 files in 18 ms
Reused 842/917 semantic nodes
Recomputed 12 paragraphs
Reflowed pages 14-16
Updated 3 references
Wrote paper.pdf in 220 ms
```

That is the dream: the engine tells you what happened instead of burping “rerun to get
cross-references right” like a haunted thermostat.

---

# 9. Layout priorities

Mosaic should expose layout constraints explicitly.

## 9.1 Constraint classes

```text
hard
strong
normal
weak
aesthetic
```

Example:

```mos
#set layout(
  widows: avoid(priority: strong),
  orphans: avoid(priority: strong),
  heading_keep_with_next: hard,
  figure_near_reference: strong,
  balanced_pages: weak,
)
```

## 9.2 Hard constraints

Must never be violated:

```text
- no content overlap
- no content outside page bounds unless explicitly allowed
- labels must be unique
- references must resolve
- physical page size respected
- counters deterministic
```

## 9.3 Strong constraints

Violate only with warning:

```text
- keep heading with following paragraph
- avoid widows/orphans
- keep figure near anchor
- keep table row together
```

## 9.4 Weak constraints

Used for visual quality:

```text
- minimize whitespace
- balance facing pages
- avoid awkward hyphenation
- prefer float at top rather than bottom
```

The engine should report compromises:

```text
Warning:
Figure <fig:ctpa> placed 3 pages after anchor.
Reason:
No legal placement existed within 2 pages without violating hard constraints.
Suggestion:
Reduce figure height, allow bottom placement, or increase max distance.
```

That is vastly better than LaTeX’s classic “float too large” and then the figure teleports to the
document’s afterlife.

---

# 10. Float model

Floats need first-class semantics.

```mos
#figure(
  image("scan.png"),
  caption: "CTPA showing embolus.",
  placement: near(anchor, max-distance: 2 pages),
  allowed: [top, bottom, page],
  priority: high,
) <fig:scan>
```

Internally:

```rust
struct FloatConstraint {
    anchor: NodeId,
    allowed_positions: Vec<FloatPosition>,
    max_distance: Option<PageDistance>,
    priority: Priority,
    can_split: bool,
}
```

A float solver can then optimize:

```text
cost = distance_from_anchor
     + whitespace_penalty
     + ordering_penalty
     + priority_violation_penalty
```

This turns float placement into an explicit optimization problem instead of a cage fight with
`[htbp!]`.

---

# 11. Tables

Tables are awful. Obviously. They are spreadsheets pretending to be typography.

Mosaic should treat tables as layout objects with multiple strategies:

```mos
#table(
  columns: [auto, 1fr, 2fr, fixed(30mm)],
  header: true,
  split: rows,
  repeat_header: true,
)[
  ...
]
```

Table solver priorities:

```text
1. preserve cell content
2. respect fixed widths
3. satisfy min/max intrinsic widths
4. split rows only if allowed
5. repeat headers on page break
6. minimize ugly wrapping
```

The table engine should support:

```text
- intrinsic sizing
- fixed sizing
- fractional sizing
- multipage tables
- repeated headers
- row groups
- footnotes
- captions
- accessibility metadata
```

Do not make table layout an afterthought. That is how every report generator becomes a war crime.

---

# 12. Bibliography system

Bibliography must be built in.

No separate BibTeX/Biber execution.

Mosaic should support:

```text
- CSL styles
- BibLaTeX import
- BibTeX import
- DOI metadata import optionally
- numeric citations
- author-year citations
- footnote citations
- sorted bibliographies
- citation clusters
```

Example:

```mos
#set bibliography(
  source: "references.bib",
  style: "vancouver"
)

Pulmonary embolism guidelines recommend risk stratification @konstantinides2020.
```

Print bibliography:

```mos
#bibliography()
```

Internally:

```text
Citation node
  depends on:
    citation key
    bibliography database
    citation style
    citation order
```

For numeric styles, citation numbers depend on first appearance order. That means citation changes
can affect later citations. Fine. Track it explicitly.

---

# 13. Indexes and glossaries

Built in.

```mos
#index("pulmonary embolism")
#glossary("CTPA", "Computed tomography pulmonary angiography")
```

Render:

```mos
#index()
#glossary()
```

No external `makeindex`. No auxiliary goblin dance.

---

# 14. Package system

## 14.1 Package manifest

Every project has:

```toml
# mosaic.toml

[project]
name    = "pe-report"
version = "0.1.0"
entry   = "main.mos"

[document]
language = "en"
output   = ["pdf", "html"]

[dependencies]
clinical        = "1.2"
vancouver-style = "2.0"
```

## 14.2 Lockfile

```text
mosaic.lock
```

Required for reproducible builds.

## 14.3 Package registry

Packages should contain:

```text
- functions
- styles
- templates
- assets
- bibliography styles
- layout policies
```

Packages should not have arbitrary native code execution by default.

Scripting should be sandboxed.

The default package model should be:

```text
pure package:
  deterministic
  no filesystem access except declared assets
  no network access
  cacheable

trusted package:
  optional elevated capabilities
  explicit user consent
```

Because if packages can run arbitrary code during document compilation, congratulations, you
reinvented npm but with footnotes.

---

# 15. Build system

## 15.1 Commands

Implemented (MVP 0 scaffold; see §30):

```bash
mos init
mos build
mos watch
mos check
mos fmt
mos test
mos profile
mos clean
mos package
```

Deferred to later MVPs:

```bash
mos graph    # dependency inspection (§8); MVP 5
mos bundle   # archival `.mosaicbundle` (§15.3); MVP 5
mos convert  # best-effort LaTeX import (§29); post-MVP
```

## 15.2 Build output

Default structure:

```text
project/
  main.mos
  mosaic.toml
  mosaic.lock
  refs.bib
  figures/
  styles/
  build/
    main.pdf
    main.html
    debug/
  .mosaic-cache/
```

## 15.3 Reproducible builds

A build should depend on:

```text
- source files
- package versions
- font versions
- bibliography files
- asset hashes
- engine version
- layout policy
```

`mos build --frozen` should refuse to update dependencies.

```bash
mos build --frozen
```

For archival builds:

```bash
mos bundle main.mos
```

Produces:

```text
main.mosaicbundle
```

Containing:

```text
- sources
- lockfile
- assets
- bibliography
- fonts if license permits
- engine metadata
```

No font piracy, because apparently even letters have lawyers.

---

# 16. Diagnostics

Diagnostics need to be first-class.

Example error:

```text
error[E041]: unresolved reference `fig:scan`
  main.mos:42:18
   |
42 | As shown in @fig:scan, ...
   |                  ^^^^ no figure, table, equation, or section has this label

help:
  similar labels:
    fig:ctpa-scan
    fig:wells-flow
```

Layout warning:

```text
warning[W203]: figure placed far from anchor
  main.mos:88:1
   |
88 | #figure(...)
   | ^^^^^^^^^^^

Figure <fig:ctpa> was placed 3 pages after its reference.
Reason: it is 132mm tall and no legal top/bottom placement existed earlier.
```

Performance diagnostic:

```text
note[perf]: table layout is expensive
  main.mos:120:1

This table caused 71% of layout time.
Consider fixed column widths or disabling optimal wrapping.
```

This is the sort of thing that makes users feel like the program is helping instead of cackling in
Knuthian.

---

# 17. Editor integration

Build an LSP from the beginning.

Features:

```text
- syntax highlighting
- diagnostics
- go to definition for labels
- rename label
- citation autocomplete
- figure preview
- outline
- symbol search
- live PDF preview sync
- format document
- hover docs
```

Source-to-PDF sync needs bidirectional mapping:

```text
source span ↔ rendered page region
```

Store this in a sidecar file:

```text
main.mosync
```

Or embed it into PDF metadata when possible.

---

# 18. Formatting conventions

Use an opinionated formatter.

```bash
mos fmt
```

Formatting rules:

```mos
#figure(
  image("scan.png"),
  caption: "CTPA showing segmental embolus.",
  placement: near,
) <fig:scan>
```

Not:

```mos
#figure(image("scan.png"),caption:"CTPA showing segmental embolus.",placement:near)<fig:scan>
```

We are building civilization, not minified JavaScript with page numbers.

---

# 19. Style system

Styles should cascade, but predictably.

Inspired by CSS, but with typesetting-specific semantics.

```mos
#set text(font: "Libertinus Serif", size: 11pt)

#show heading.where(level: 1): set text(size: 18pt, weight: bold)
#show figure.caption: set text(size: 9pt, style: italic)
```

Style resolution should be deterministic:

```text
document defaults
  ↓
template defaults
  ↓
package styles
  ↓
local style rules
  ↓
inline overrides
```

No arbitrary “last macro wins after expansion unless the moon is in retrograde.”

---

# 20. Template system

Templates are normal packages.

Example:

```mos
#import "@mosaic/templates/article": article

#show: article.with(
  title: "Diagnostic Reasoning in PE",
  author: "Kaj Kowalski",
  abstract: [
    This report discusses...
  ],
)
```

Templates should expose parameters, not demand users edit class-file entrails.

---

# 21. Output backends

## 21.1 PDF backend

The PDF backend should handle:

```text
- embedded fonts
- subset fonts
- hyperlinks
- bookmarks
- metadata
- tagged PDF eventually
- PDF/A support eventually
- vector graphics
- image compression
```

Use existing libraries where possible, but be prepared to write backend code. PDF is a cursed
bureaucracy in file format form.

## 21.2 HTML backend

HTML output should preserve semantics:

```html
<section>
	<h1>Methods</h1>
	<p>...</p>
	<figure>
		<img src="scan.png">
		<figcaption>...</figcaption>
	</figure>
</section>
```

Not:

```html
<div style="position:absolute; left:72.1px; top:183.7px">
```

Unless exporting fixed-layout HTML pages.

## 21.3 Debug backend

This is important.

```bash
mos build --debug-layout
```

Produces a visual layout report:

```text
- boxes
- baselines
- constraints
- dirty nodes
- float decisions
- page break costs
```

The debug backend will save your sanity. Or at least preserve enough of it to file issues
coherently.

---

# 22. The layout algorithm

## 22.1 Inline layout

Use Knuth-Plass line breaking as a base.

But modernize:

```text
- Unicode line breaking
- OpenType shaping
- language-aware hyphenation
- variable fonts eventually
- math-aware inline spacing
```

Pipeline:

```text
text
  ↓
segment into script/language runs
  ↓
shape with font engine
  ↓
produce glyph boxes/glue/penalties
  ↓
line breaking
  ↓
inline boxes
```

## 22.2 Page breaking

Classic TeX has strong paragraph line breaking. Page breaking and floats are less elegant.

Mosaic should use a cost-based regional optimizer.

Basic page break cost:

```text
cost =
  badness(remaining_space)
  + widow_orphan_penalty
  + heading_stranding_penalty
  + float_distance_penalty
  + footnote_split_penalty
  + user_constraint_penalty
```

For long documents, do not globally optimize all pages at once. That explodes.

Use regions:

```text
- section
- chapter
- explicit layout region
- bounded window around changed content
```

Reflow windows:

```text
changed paragraph on page 40
  → reflow pages 40-43
  → if overflow changes page 44, extend window
  → stop when page boundary state matches previous stable state
```

This is the exact trick: convergence by boundary stability.

## 22.3 Boundary state

Each page or region has a boundary signature:

```rust
// `BlockId` and `FloatId` are aliases over `NodeId` in the eventual
// design; the MVP 0 layout scaffold uses `Option<NodeId>` and
// `Vec<NodeId>` directly. `counter_state` and `footnote_state` are
// deferred to MVP 1 (references and counters) and MVP 3 (figures and
// floats) respectively.
struct BoundaryState {
    next_block: BlockId,
    pending_floats: Vec<FloatId>,
    counter_state: CounterState,
    footnote_state: FootnoteState,
}
```

If after recomputation the boundary state matches the cached state, later pages can be reused.

This is how you avoid recompiling the whole damn document because one sentence got longer.

---

# 23. Fixpoint model

Some values depend on final layout:

```text
page references
TOC page numbers
list of figures
index locators
```

The engine performs internal fixpoint iteration:

```rust
loop {
    let changes = recompute_dirty_nodes();

    if changes.is_empty() {
        break;
    }

    if iteration_count > MAX_ITERATIONS {
        emit_nonconvergence_diagnostic();
        break;
    }
}
```

But dirty nodes are targeted.

Example:

```text
Iteration 1:
  page numbers change in TOC

Iteration 2:
  TOC length changes from 2 pages to 3 pages

Iteration 3:
  page numbers shift again

Iteration 4:
  stable
```

The user sees:

```text
Stabilized in 4 internal layout iterations.
```

Not four separate CLI runs like a peasant.

---

# 24. Determinism

Builds must be deterministic.

That means:

```text
- stable iteration order
- deterministic hash maps where output order matters
- pinned package versions
- pinned font resolution
- no network during build unless explicitly enabled
- no system time unless declared
```

If a document uses `today()`, that must be tracked:

```mos
#set document(date: today())
```

For reproducible mode:

```bash
mos build --reproducible
```

Either freeze the date or error.

---

# 25. Scripting model

You need programmability, but not madness.

Options:

## Option A: custom small language

Pros:

```text
- controlled
- deterministic
- tailored to documents
```

Cons:

```text
- you have to build a language
- humans will complain regardless
```

## Option B: embedded Rhai / Starlark / WASM

Best serious choice:

```text
- core document language remains declarative
- advanced packages can use sandboxed WASM
- capabilities are explicit
```

Suggested model:

```text
Mosaic native expression language for normal templates
WASM plugin API for advanced extensions
```

Plugin manifest:

```toml
[plugin]
name    = "chemical-formula"
version = "1.0.0"

[capabilities]
filesystem    = false
network       = false
deterministic = true
```

The engine calls plugins through stable ABI-like functions:

```rust
trait MosaicPlugin {
    fn transform_node(&self, input: Node) -> Result<Node>;
    fn provide_layout(&self, node: Node) -> Result<LayoutObject>;
}
```

---

# 26. Project conventions

Recommended project layout:

```text
my-paper/
  mosaic.toml
  mosaic.lock
  main.mos
  sections/
    introduction.mos
    methods.mos
    discussion.mos
  figures/
    wells-flow.svg
    ctpa.png
  data/
    results.csv
  refs/
    references.bib
  styles/
    journal.mos
  build/
  .mosaic-cache/
```

Import sections:

```mos
#include "sections/introduction.mos"
#include "sections/methods.mos"
#include "sections/discussion.mos"
```

Assets are addressed relative to project root unless otherwise stated.

---

# 27. Versioning and compatibility

Mosaic files should declare language version:

```mos
#set mosaic(version: "0.1")
```

Or in manifest:

```toml
[project]
language-version = "0.1"
```

Breaking changes are gated by language version.

No repeating LaTeX’s eternal curse where ancient documents must compile forever under every new
engine while everyone pretends this is noble rather than a hostage situation.

---

# 28. Testing documents

Documents and packages need tests.

```bash
mos test
```

Test types:

```text
- syntax tests
- semantic tests
- reference resolution tests
- layout snapshot tests
- PDF metadata tests
- visual regression tests
```

Example:

```mos
#test "figure reference resolves" {
  let doc = compile[
    See @fig:test.

    #figure(rect(width: 10mm, height: 10mm), caption: "Test") <fig:test>
  ]

  assert(doc.references["fig:test"].text == "Figure 1")
}
```

Visual tests should compare layout trees, not raw PDFs. Raw PDF diffs are a prank from Satan.

---

# 29. Migration path

You need migration, but do not make full LaTeX compatibility the core promise.

Support:

```text
- Markdown import
- Pandoc JSON import
- BibTeX/BibLaTeX import
- CSL styles
- limited LaTeX math import
- image/table import
```

Maybe:

```bash
mos convert paper.tex --best-effort
```

But the output should be Mosaic-native.

Do not support arbitrary LaTeX packages. That path leads straight back into the swamp with better
syntax highlighting.

---

# 30. MVP roadmap

## MVP 0: Core compiler skeleton

Goal:

```text
.mos → document graph → simple PDF
```

Features:

```text
- parser                ✅  mosaic-parse: headings, paragraphs,
                            inline emphasis/strong/code, `#set` blocks
- sections              ✅  lowered as NodeKind::Section under the document root
- paragraphs            ✅
- bold/italic/code      ✅  inline emphasis (NodeKind::Emphasis), strong
                            (NodeKind::Strong), and code (NodeKind::Raw)
- basic page layout     ⏳
- PDF output            ⏳  mosaic-pdf::emit is still a stub
- diagnostics           ✅  recoverable; rendered with file:line:col + carets
                            by `mos check` (manifest §16)
```

No floats yet. No bibliography yet. No heroic bullshit.

## MVP 1: References and counters

Add:

```text
- labels
- references
- section numbering
- figure numbering
- equation numbering
- internal fixpoint loop
```

## MVP 2: Real text layout

Add:

```text
- font loading
- shaping
- line breaking
- hyphenation
- paragraph layout cache
```

## MVP 3: Figures and floats

Add:

```text
- images
- captions
- float placement constraints
- list of figures
- debug float diagnostics
```

## MVP 4: Bibliography

Add:

```text
- BibTeX import
- CSL styles
- citation clusters
- bibliography rendering
```

## MVP 5: Incremental builds

Add:

```text
- dependency graph
- stable node IDs
- dirty-node invalidation
- persistent cache
- watch mode
```

## MVP 6: Editor integration

Add:

```text
- LSP
- live preview
- source/PDF sync
```

This sequence matters. Do not start with package systems and plugins. That is how projects die in a
beautiful architecture document and never render “hello world.”

---

# 31. Minimal Rust architecture

Something like:

```rust
pub struct Compiler {
    parser: Parser,
    resolver: Resolver,
    layout_engine: LayoutEngine,
    backend: BackendRegistry,
    cache: Cache,
}

impl Compiler {
    pub fn compile(&mut self, input: CompileInput) -> Result<CompileOutput> {
        let syntax = self.parser.parse_project(&input)?;
        let semantic = self.resolver.lower(syntax)?;
        let resolved = self.resolver.resolve(semantic)?;

        let layout = self
            .layout_engine
            .layout_incremental(resolved, &mut self.cache)?;

        self.backend.emit_all(layout, input.outputs)
    }
}
```

Core data flow:

```rust
pub struct CompileInput {
    pub entry: PathBuf,
    pub project_root: PathBuf,
    pub outputs: Vec<OutputKind>,
    pub mode: CompileMode,
}

pub enum CompileMode {
    Check,
    Build,
    Watch,
    Reproducible,
}
```

Diagnostics:

```rust
pub struct Diagnostic {
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
    pub span: Option<SourceSpan>,
    pub notes: Vec<DiagnosticNote>,
    pub suggestions: Vec<Suggestion>,
}
```

No panics for user errors. Ever. A typesetter should not crash because someone forgot a bracket. It
should complain with dignity, which is apparently an advanced feature.

---

# 32. Cache design

Cache keys should include:

```text
- node content hash
- style hash
- available layout region
- font metrics hash
- engine version
- package versions
```

Example paragraph layout cache key:

```rust
// `Language` is deferred to MVP 2 (real text layout); MVP 0 keys
// paragraphs without locale and revisits hyphenation/CJK then.
struct ParagraphCacheKey {
    node_hash: ContentHash,
    style_hash: ContentHash,
    width: Abs,
    font_set_hash: ContentHash,
    language: Language,
}
```

Cached output:

```rust
struct ParagraphLayoutCacheEntry {
    lines: Vec<LineBox>,
    height: Abs,
    baseline_positions: Vec<Abs>,
    dependencies: Vec<DepId>,
}
```

This lets you reuse paragraph layout across builds.

---

# 33. Page reflow algorithm

Pseudo-code:

```rust
fn reflow_from(start_page: PageIndex, old_pages: &[Page]) -> Vec<Page> {
    let mut page_index = start_page;
    let mut state = old_pages
        .get(start_page)
        .map(|p| p.input_boundary.clone())
        .unwrap_or_default();

    loop {
        let new_page = layout_one_page(state);

        let old_next_boundary = old_pages.get(page_index).map(|p| &p.output_boundary);

        if Some(&new_page.output_boundary) == old_next_boundary {
            reuse_remaining_pages_from(page_index + 1);
            break;
        }

        state = new_page.output_boundary.clone();
        store(new_page);
        page_index += 1;
    }
}
```

This is the magic sauce for incremental pagination.

If page 12 changes but page 15’s boundary state matches the old build, pages 16 onward are reusable.

---

# 34. Handling non-convergence

Some documents can oscillate.

Example:

```text
TOC is 2 pages → section starts page 3
TOC is 3 pages → section starts page 4
page reference text changes → TOC shrinks back
```

The engine should detect this.

Strategy:

```text
- keep hashes of global layout states
- detect repeated states
- choose stable fallback policy
- emit diagnostic
```

Fallback:

```text
- freeze page-reference width
- reserve worst-case width
- use previous stable value
- require user intervention for pathological case
```

Diagnostic:

```text
error[E301]: layout did not converge after 12 iterations

The page number width of TOC entries caused oscillation.
Suggestion:
Reserve fixed-width page numbers in the table of contents:
#set toc(page-number-width: 3ch)
```

There. Helpful. Unlike “Label(s) may have changed,” which is just TeX’s way of shrugging in Latin.

---

# 35. Design non-goals

Important.

Mosaic should **not** try to be:

```text
- fully LaTeX-compatible
- a general programming language
- a web browser
- a desktop publishing GUI
- a Word clone
- a CSS clone
- a markdown-only toy
```

The core identity:

> A programmable, semantic, incremental, constraint-based typesetting compiler.

That’s the lane.

Stay in it.

---

# 36. What to prioritize

In order:

```text
1. semantic correctness
2. deterministic builds
3. good diagnostics
4. high-quality typography
5. incremental performance
6. extensibility
7. multiple output formats
8. migration tooling
```

Notably, “extensibility” is not number one.

That is deliberate. If extensibility comes before semantic clarity, you recreate TeX.
Congratulations, you have built the Minotaur and the maze at the same time.

---

# 37. The killer feature

The killer feature is not syntax.

It is this:

```text
Change one sentence.
Rebuild in 80 ms.
References correct.
Floats stable.
PDF preview updates instantly.
Diagnostics explain layout compromises.
```

That is what would make people switch.

Not “we have prettier macros.” Nobody sane cares. Sadly, many people are not sane, but still.

---

# 38. The one-sentence proposal

**Mosaic is a Rust-based, semantic, incremental typesetting compiler that represents documents as
dependency graphs, solves layout using explicit constraints and priorities, performs internal
fixpoint stabilization, and emits reproducible PDF/HTML/EPUB outputs without external rebuild
rituals.**

That is the thing.

The hard part is not inventing the idea.

The hard part is refusing compatibility temptations long enough to avoid rebuilding LaTeX’s cursed
little palace with better bricks.
