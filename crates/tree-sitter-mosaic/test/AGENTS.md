# TREE-SITTER TEST KNOWLEDGE BASE

## OVERVIEW

`test/` validates editor grammar behavior. It is not compiler parser truth.

## STRUCTURE

```text
test/
├── corpus/    # tree-sitter parse corpus with expected S-expressions
├── highlight/ # highlight query fixtures
└── inputs/    # standalone .mos parser/highlight inputs
```

## CONVENTIONS

- Corpus files use Tree-sitter sections: `==== name ====`, source, `---`, expected tree.
- Query behavior starts in `../queries/`; copied Zed query files live elsewhere.
- Grammar source is `../grammar.js` plus `../src/scanner.c`; generated parser artifacts are not the
  place to fix tests.

## COMMANDS

```bash
npm test
npm run parse -- test/inputs/<file>.mos
npm run highlight -- test/highlight/<file>.mos
```

## ANTI-PATTERNS

- Do not change compiler semantics because a Tree-sitter fixture wants nicer syntax.
- Do not hand-edit generated `../src/parser.c` to satisfy tests.
- Do not run the playground unless explicitly asked; it is interactive/server-ish.
