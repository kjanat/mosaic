# Zed Mosaic Language Config Review

Date: 2026-06-16

Zed upstream checked against commit:
[`2408640e5f70e65732ef4af73badd4e9e6fe9c2f`](https://github.com/zed-industries/zed/commit/2408640e5f70e65732ef4af73badd4e9e6fe9c2f)

Reviewed local files:

- `crates/zed-mosaic/languages/mosaic/config.toml`
- `crates/zed-mosaic/languages/mosaic/semantic_token_rules.json`
- `crates/zed-mosaic/languages/mosaic/tasks.json`
- Related query glue: `crates/zed-mosaic/languages/mosaic/runnables.scm`, `overrides.scm`,
  `highlights.scm`

## Summary

The Zed files mostly use valid Zed formats. Main problem is not syntax validity; main problem is
product-truth drift. Some editor features describe either Tree-sitter-only syntax or future
compiler/LSP behavior that Mosaic does not currently ship.

Ratings:

| File                        | Zed validity | Current usefulness | Notes                                                                                 |
| --------------------------- | ------------ | ------------------ | ------------------------------------------------------------------------------------- |
| `config.toml`               | 8/10         | 7/10               | Valid fields; comments/math affordances are ahead of compiler truth.                  |
| `semantic_token_rules.json` | 9/10         | 2/10               | Valid schema; inert until `mos-lsp` emits matching semantic tokens.                   |
| `tasks.json`                | 9/10         | 8/10               | Valid task templates; tags make tasks runnable-driven, not generic task-list entries. |

## Zed Expectations

Zed language extensions use a language directory containing `config.toml`, query files, optional
`tasks.json`, and optional `semantic_token_rules.json`.

Read upstream:

- Language metadata docs:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/docs/src/extensions/languages.md#L15-L48>
- `LanguageConfig` source:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/crates/language_core/src/language_config.rs#L30-L146>
- Extension language loading, including semantic rules and tasks:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/crates/extension_host/src/extension_host.rs#L1344-L1378>

Relevant Zed docs excerpt for Tree-sitter highlights:

> This query marks strings, object keys, and numbers for highlighting. The following is the full
> list of captures supported by themes.

Source:
<https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/docs/src/extensions/languages.md#L98-L132>

Important captures for Mosaic today:

| Capture                         | Use in Mosaic                                  |
| ------------------------------- | ---------------------------------------------- |
| `@keyword`                      | `#set`, `#image`, `#figure`, directive syntax. |
| `@title`                        | Headings.                                      |
| `@emphasis`, `@emphasis.strong` | Inline emphasis and strong text.               |
| `@text.literal`                 | Inline code, code/pre bodies.                  |
| `@label`                        | Labels.                                        |
| `@link_text`, `@link_uri`       | References and label names.                    |
| `@punctuation.*`                | Markers, brackets, delimiters.                 |
| `@string.escape`                | Mosaic escapes.                                |

## Mismatch 1: Semantic Token Rules Are Valid But Inert

Local file:

- `crates/zed-mosaic/languages/mosaic/semantic_token_rules.json`

What exists now:

- The file defines custom token types like `mosaicDirective`, `mosaicHeading`, `mosaicLabel`, and
  `mosaicListMarker`.
- Zed will load the file automatically when present in the language directory.
- Current `mos-lsp` does not advertise `semanticTokensProvider` or serve semantic token requests.
- Therefore no LSP semantic token with token type `mosaicDirective` etc. is ever produced, so these
  rules do not affect rendering.

Why this matters:

- This file does not style Tree-sitter captures. Tree-sitter styling lives in `highlights.scm`.
- Semantic token rules only remap tokens that come from the language server.

What is needed to make it work:

1. Add semantic-token support to `mos-lsp`.
2. Advertise `semanticTokensProvider` in LSP initialize capabilities.
3. Define a semantic-token legend whose token type strings exactly match `semantic_token_rules.json`
   values such as `mosaicDirective`.
4. Implement `textDocument/semanticTokens/full` at minimum.
5. Compute token ranges from parser spans or lowered semantic nodes.
6. Add tests for legend stability, range encoding, and representative Mosaic syntax.
7. Decide precedence with Tree-sitter highlighting and user setting `semantic_tokens`.

Where to read more:

- Zed loads extension `semantic_token_rules.json`:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/crates/extension_host/src/extension_host.rs#L1352-L1358>
- Zed `SemanticTokenRules` schema source:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/crates/settings_content/src/project.rs#L256-L300>
- Built-in Zed Rust semantic token rules example:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/crates/grammars/src/rust/semantic_token_rules.json>
- Local LSP server entrypoint: `crates/mos-lsp/src/server.rs`
- Local semantic-token rules: `crates/zed-mosaic/languages/mosaic/semantic_token_rules.json`
- Local Tree-sitter highlights that already style similar concepts:
  `crates/zed-mosaic/languages/mosaic/highlights.scm`

Recommended action now:

- Keep semantic rules only if semantic-token work is imminent.
- Otherwise remove the file or add a comment elsewhere documenting that it is reserved for future
  `mos-lsp` semantic tokens.
- Prefer `highlights.scm` for current editor colors.

## Mismatch 2: Comment Editing Is Ahead Of Compiler Truth

Local file:

- `crates/zed-mosaic/languages/mosaic/config.toml`

Relevant local config:

```toml
line_comments = ["// "]

[documentation_comment]
start    = "/**"
end      = "*/"
prefix   = " * "
tab_size = 1

[block_comment]
start    = "/*"
end      = "*/"
prefix   = " * "
tab_size = 1
```

What exists now:

- Zed config fields are valid.
- Tree-sitter grammar recognizes `//...` and `/*...*/` comments.
- Zed `ToggleComments` will insert `//`.
- Current `mos-parse` knowledge base says comments are not implemented in compiler syntax.

Why this matters:

- Zed can help users create syntax that the editor grammar treats as comments.
- The compiler may treat that same text as prose or unsupported syntax.
- That creates an editor/compiler contract breach.

What is needed to make it work:

1. Decide whether comments are shipped Mosaic language syntax.
2. If yes, implement comment skipping or preservation in `mos-parse`.
3. Ensure comments are accepted at top level, between blocks, and inside directive-expression
   contexts only where intended.
4. Add parser tests for line comments, block comments, comments near headings, comments near lists,
   and comments inside directive argument lists if supported.
5. Add lowerer tests proving comments do not produce document nodes.
6. Update user docs and `crates/mos-parse/AGENTS.md` current grammar list.
7. If no, remove `line_comments`, `[block_comment]`, `[documentation_comment]`, and related bracket
   entries from Zed config.

Where to read more:

- Zed `line_comments`, `block_comment`, `documentation_comment` config fields:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/crates/language_core/src/language_config.rs#L83-L90>
- Zed language metadata docs for `line_comments`:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/docs/src/extensions/languages.md#L15-L48>
- Local Tree-sitter comment grammar: `crates/tree-sitter-mosaic/grammar.js`
- Local compiler parser knowledge: `crates/mos-parse/AGENTS.md`
- Local compiler parser source: `crates/mos-parse/src/`
- Local Zed config: `crates/zed-mosaic/languages/mosaic/config.toml`

Recommended action now:

- If comments are not shipping next, remove comment toggling from `config.toml` to avoid false
  affordance.
- If comments are shipping soon, keep Zed config and implement compiler support in the same feature
  slice.

## Mismatch 3: Math Editing Affordance Is Ahead Of Compiler Truth

Local file:

- `crates/zed-mosaic/languages/mosaic/config.toml`

Relevant local config:

```toml
brackets = [
  { start = "$", end = "$", close = true, newline = false, not_in = ["comment", "string", "raw"] },
]
```

What exists now:

- Zed bracket config is valid.
- Tree-sitter-side files refer to `inline_math`.
- Current parser knowledge says math is not implemented.

Why this matters:

- Zed will auto-close `$...$` as if math is a first-class syntax form.
- Users can infer that inline math is supported by Mosaic.
- Compiler truth says otherwise today.

What is needed to make it work:

1. Decide concrete math syntax and shipped scope.
2. Implement inline math parsing in `mos-parse` with spans.
3. Add a semantic inline node or preserve math as a typed inline in `mos-core` / `mos-eval`.
4. Add layout/rendering behavior or a clear diagnostic if parsed but not renderable.
5. Keep Tree-sitter grammar, `highlights.scm`, `overrides.scm`, and compiler parser aligned.
6. Add tests for `$x$`, escaped dollars, unmatched dollar recovery, and interaction with code spans.

Where to read more:

- Zed bracket pair config source:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/crates/language_core/src/language_config.rs#L414-L439>
- Local editor overrides mentioning `inline_math`:
  `crates/zed-mosaic/languages/mosaic/overrides.scm`
- Local Tree-sitter grammar: `crates/tree-sitter-mosaic/grammar.js`
- Local parser knowledge saying math is not implemented: `crates/mos-parse/AGENTS.md`

Recommended action now:

- Remove `$` auto-close until compiler support lands, unless the intent is deliberately
  future-looking editor grammar.

## Mismatch 4: Tasks Are Runnable-Tagged, Not Generic Task-List Entries

Local files:

- `crates/zed-mosaic/languages/mosaic/tasks.json`
- `crates/zed-mosaic/languages/mosaic/runnables.scm`

Relevant local task config:

```json
{
	"label": "Mosaic: Build PDF",
	"command": "mos",
	"args": ["build", "$ZED_FILENAME"],
	"cwd": "$ZED_DIRNAME",
	"tags": ["mosaic-build"],
	"use_new_terminal": false,
	"allow_concurrent_runs": false,
	"reveal": "always"
}
```

What exists now:

- `tasks.json` is valid Zed task JSON.
- `$ZED_FILENAME` and `$ZED_DIRNAME` are valid task variables.
- `runnables.scm` emits `mosaic-build` and `mosaic-build-open-pdf` tags on `source_file`.
- Zed source says tags attach a task to runnable tags and remove it from other UI.

Why this matters:

- This is correct if the intended UI is run buttons in Mosaic buffers.
- This is not correct if the intended UI is also normal task-palette visibility.

What is needed to make it work as run buttons:

1. Keep matching tags in `tasks.json` and `runnables.scm`.
2. Keep `@run` capture in `runnables.scm` at the desired UI location.
3. Verify Zed shows one or two runnable actions at the top of a `.mos` file.
4. Keep task variables aligned with CLI behavior.

What is needed to make it work as normal tasks too:

1. Add duplicate untagged tasks, or remove `tags` if run buttons are not needed.
2. Keep runnable-tagged tasks only for run-button use.
3. Use different labels if both tagged and untagged variants exist, to avoid confusion.

<details><summary><code>ZED_VARIABLE_NAME_PREFIX</code></summary>

```rs
fn from_str(s: &str) -> Result<Self, Self::Err> {
    let without_prefix = s.strip_prefix(ZED_VARIABLE_NAME_PREFIX).ok_or(())?;
    let value = match without_prefix {
        "FILE" => Self::File,
        "FILENAME" => Self::Filename,
        "RELATIVE_FILE" => Self::RelativeFile,
        "RELATIVE_DIR" => Self::RelativeDir,
        "DIRNAME" => Self::Dirname,
        "STEM" => Self::Stem,
        "WORKTREE_ROOT" => Self::WorktreeRoot,
        "SYMBOL" => Self::Symbol,
        "RUNNABLE_SYMBOL" => Self::RunnableSymbol,
        "SELECTED_TEXT" => Self::SelectedText,
        "LANGUAGE" => Self::Language,
        "ROW" => Self::Row,
        "COLUMN" => Self::Column,
        "MAIN_GIT_WORKTREE" => Self::MainGitWorktree,
        "GIT_SHA" => Self::GitSha,
        "GIT_SHA_SHORT" => Self::GitShaShort,
        "GIT_REPOSITORY_NAME" => Self::GitRepositoryName,
        "GIT_REPOSITORY_PATH" => Self::GitRepositoryPath,
        "GIT_REF" => Self::GitRef,
        _ => {
            if let Some(custom_name) = without_prefix.strip_prefix(ZED_CUSTOM_VARIABLE_NAME_PREFIX)
            {
                Self::Custom(Cow::Owned(custom_name.to_owned()))
            } else {
                return Err(());
            }
        }
    };
    Ok(value)
}
```

</details>

Where to read more:

- Zed task template fields:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/crates/task/src/task_template.rs#L24-L75>
- Zed task `tags` behavior source comment:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/crates/task/src/task_template.rs#L61-L66>
- Zed task file name source:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/crates/task/src/task_template.rs#L141-L146>
- Zed task variables source:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/crates/task/src/task.rs#L154-L224>
- Zed runnable docs:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/docs/src/extensions/languages.md#L354-L394>
- Local runnable query: `crates/zed-mosaic/languages/mosaic/runnables.scm`
- Local task file: `crates/zed-mosaic/languages/mosaic/tasks.json`
- Local CLI build/open behavior: `crates/mos/src/main.rs`

Recommended action now:

- Keep current task setup if run buttons were the intent.
- Add untagged duplicate tasks only if task-list discoverability matters.

## Mismatch 5: Tree-Sitter Highlights Already Cover Most Semantic Rule Intent

Local files:

- `crates/zed-mosaic/languages/mosaic/highlights.scm`
- `crates/zed-mosaic/languages/mosaic/semantic_token_rules.json`

What exists now:

- `highlights.scm` uses Zed-supported captures like `@keyword`, `@title`, `@emphasis`, `@label`,
  `@link_text`, `@link_uri`, and `@punctuation.*`.
- `semantic_token_rules.json` repeats similar styling intent using custom LSP token names.
- Without semantic-token support in `mos-lsp`, only `highlights.scm` has effect.

Why this matters:

- Two styling systems encode the same design intent.
- One works now; the other waits for LSP semantic tokens.
- If both eventually run, styling precedence may surprise unless token rules are deliberately
  narrower than Tree-sitter captures.

What is needed to make both coexist:

1. Treat Tree-sitter highlights as baseline lexical styling.
2. Use semantic tokens only for information Tree-sitter cannot know, such as resolved label
   declaration vs reference, unresolved references, semantic directive targets, or bibliography key
   validity.
3. Rename token types to stable semantic names before shipping them as LSP legend strings.
4. Add visual regression checks or manual test fixtures for overlap.

Where to read more:

- Zed highlight capture docs and supported captures:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/docs/src/extensions/languages.md#L98-L132>
- Zed semantic token rules schema:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/crates/settings_content/src/project.rs#L288-L300>
- Local Tree-sitter highlights: `crates/zed-mosaic/languages/mosaic/highlights.scm`
- Local semantic rules: `crates/zed-mosaic/languages/mosaic/semantic_token_rules.json`

Recommended action now:

- Keep styling in `highlights.scm`.
- Reserve semantic tokens for true semantic facts once `mos-lsp` can compute them.

## Valid Things Worth Keeping

These are not mismatches.

| Local feature                                                     | Verdict | Reason                                             |
| ----------------------------------------------------------------- | ------- | -------------------------------------------------- |
| `path_suffixes = ["mos"]`                                         | Keep    | Correct language association.                      |
| `code_fence_block_name = "mos"`                                   | Keep    | Useful for Markdown injections.                    |
| `modeline_aliases = ["mos"]`                                      | Keep    | Valid Zed language matcher field.                  |
| `tab_size = 2`, `hard_tabs = false`, `soft_wrap = "editor_width"` | Keep    | Valid editor policy for prose-like syntax.         |
| `unordered_list`, `ordered_list`, `rewrap_prefixes`               | Keep    | Matches shipped list syntax and Zed config source. |
| `tasks.json` command `mos build`                                  | Keep    | Matches current CLI.                               |
| `tasks.json` command `mos build --open`                           | Keep    | Current CLI supports `--open`.                     |

Where to read more:

- Zed list config fields:
  <https://github.com/zed-industries/zed/blob/2408640e5f70e65732ef4af73badd4e9e6fe9c2f/crates/language_core/src/language_config.rs#L92-L103>
- Local parser list support: `crates/mos-parse/src/list.rs`
- Local CLI `--open`: `crates/mos/src/main.rs`

## Suggested Next Slices

1. Remove or document inert `semantic_token_rules.json`.
2. Decide whether comments are real Mosaic syntax; align compiler and Zed config either way.
3. Remove `$` auto-close or implement inline math.
4. Keep tagged tasks if run-button UX is desired; add untagged copies only if task palette
   discoverability matters.
5. Add a small Zed fixture note or manual QA checklist for `.mos` files: highlighting, comment
   toggle, list continuation, run buttons, build/open.
