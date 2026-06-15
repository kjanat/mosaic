# OPENCODE TOOLING KNOWLEDGE BASE

## OVERVIEW

`.opencode/` is a Bun/TypeScript helper workspace for OpenCode plugin tools. Not Rust compiler code.

## WHERE TO LOOK

| Task          | Location                  | Notes                                                  |
| ------------- | ------------------------- | ------------------------------------------------------ |
| Root tooling  | `../package.json`         | Bun workspace, dprint, tombi, runner.                  |
| Package deps  | `package.json`            | Uses root catalog deps.                                |
| TS config     | `tsconfig.json`           | Strict, no emit.                                       |
| Project tools | `tools/github_project.ts` | GitHub Project 5 issue creation, field/status helpers. |
| PR tools      | `tools/github_pr.ts`      | PR classify/view/triage helpers.                       |

## TOOLING RULES

- Runtime is Bun/TypeScript, via root package workspace.
- Root dev tooling includes local `dprint`, `tombi`, and `runner-run`; `just setup` installs runner
  commands.
- Tools are OpenCode plugin tools using `@opencode-ai/plugin`.
- GitHub auth comes from `GITHUB_TOKEN`, `GH_TOKEN`, or `gh auth token`.
- Defaults target Mosaic: owner `kjanat`, repo `mosaic`, Project 5.
- Project fields/statuses/areas/phases are encoded in tool option lists;\
  keep them aligned with GitHub Project 5.
- Use `github_project_create_issue` for managed planning work: it creates the issue, adds it to
  Project 5, and sets status/sprint/priority/area/phase/type/size/estimate in one tool call.

## MUTATION CAUTION

- Tools can mutate GitHub Project fields, PR metadata, and issue/project membership.
- Prefer read tools (`list`, `view`, `triage`) before write tools.
- Keep GraphQL timeouts and item limits explicit; avoid unbounded project queries.

## ANTI-PATTERNS

- Do not treat `.opencode/` as part of Cargo workspace verification.
- Do not add compiler behavior here; this is agent/tool glue.
- Do not rename Project 5 fields/options in code without matching the actual GitHub project.
