# OPENCODE TOOLS KNOWLEDGE BASE

## OVERVIEW

`tools/` contains Bun/TypeScript OpenCode plugin tools for Mosaic GitHub planning and PR triage.
These can mutate live GitHub state. Tiny cave, live spear.

## WHERE TO LOOK

| Task            | Location            | Notes                                        |
| --------------- | ------------------- | -------------------------------------------- |
| Project 5 tools | `github_project.ts` | Issues, fields, status/sprint/estimate.      |
| PR helper tools | `github_pr.ts`      | PR view/list/classify/triage/related issues. |

## CONVENTIONS

- Runtime is Bun with strict TypeScript from parent `.opencode` workspace.
- Auth order: `GITHUB_TOKEN`, `GH_TOKEN`, then `gh auth token`.
- Defaults target Mosaic Project 5. Keep owner/repo/project defaults obvious.
- Field option lists are a code mirror of real Project 5 fields; update code only with project
  change.
- Prefer read tools before mutating tools.

## MUTATION CAUTION

- `github_project_create_issue` creates a repo issue, adds it to Project 5, and sets planning
  fields.
- `github_project_set_*` tools mutate existing project items.
- `claude-code` field may only hold a cloud VM hosted session URL from user input or issue/PR body.

## ANTI-PATTERNS

- Do not use these tools for local repo state or compiler behavior.
- Do not stuff notes into `claude-code`; schema forbids it.
- Do not make broad, unbounded GraphQL queries; keep limits explicit.
