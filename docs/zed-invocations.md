# Zed invocation metadata

The Mosaic Zed extension marks the processes it launches with fixed environment values. These values
describe how the process was started. They do not enable telemetry: Mosaic currently has no
collector, telemetry log, or telemetry transport, and `mos` / `mos-lsp` do not consume the markers.

## Markers

| Launch                     | `MOS_INVOCATION_SOURCE` | `MOS_ZED_TASK`       |
| -------------------------- | ----------------------- | -------------------- |
| Mosaic: Build PDF          | `zed-task`              | `build-pdf`          |
| Mosaic: Build and Open PDF | `zed-task`              | `build-open-pdf`     |
| Mosaic language server     | `zed-lsp`               | Empty (no task kind) |

The two [task templates](../crates/zed-mosaic/languages/mosaic/tasks.json) set literal `env`
overrides using [Zed's task format](https://zed.dev/docs/tasks). Their existing file argument,
working directory, runnable tags, and viewer behavior remain the same. A user-supplied task that
replaces either template must include these markers if it needs the same attribution.

The [language-server launcher](../crates/zed-mosaic/src/lib.rs) adds `zed-lsp` after binary
discovery, covering configured, PATH, cached, and downloaded binaries. It replaces inherited
invocation markers and explicitly sets task-kind markers to an empty value. Zed merges the shell
environment with the extension's overrides, so omitting a key would retain a stale value. Inherited
alternate spellings receive the same overrides to avoid conflicting values on Windows. All other
shell environment entries remain available to the server for normal execution. Zed's user-configured
binary environment overrides can still replace these defaults.

## Boundary for future opt-in telemetry

Any future collection needs explicit user opt-in, separately from invocation attribution. Marker
presence is neither consent nor proof of origin: users, tasks, and parent processes can set these
variables themselves. Unknown source/task values must be omitted or mapped to a fixed `unknown`
category; arbitrary strings must not enter telemetry. Task kind is only meaningful with source
`zed-task`; ignore it for every other source and treat an empty value as absent.

The proposed aggregate signals are limited to invocation source, task kind, CLI version, exit
status, duration, diagnostic codes, and OS/architecture. Diagnostic messages, spans, source
snippets, command arguments, and the complete environment are outside that boundary. Any additional
fields need a separate design and privacy review before collection is implemented.

Keep these execution-context values out of telemetry, including derived identifiers or hashes:

- Paths and filenames: `ZED_FILE`, `ZED_FILENAME`, `ZED_DIRNAME`, `ZED_WORKTREE_ROOT`,
  `ZED_MAIN_GIT_WORKTREE`, `ZED_RELATIVE_FILE`, `ZED_RELATIVE_DIR`, `ZED_GIT_REPOSITORY_PATH`.
- Content and symbols: `ZED_SELECTED_TEXT`, `ZED_SYMBOL`, `ZED_RUNNABLE_SYMBOL`, document text.
- Project fingerprints: `ZED_GIT_REPOSITORY_NAME`, `ZED_GIT_SHA`, `ZED_GIT_SHA_SHORT`,
  `ZED_GIT_REF`.
- Credentials, custom environment variables, and any other fields outside the aggregate allowlist.

Tasks still need file paths to build documents, and language servers still need their normal shell
environment. Passing that execution context to the local process does not make it telemetry data.
This contract defines a future collector's allowlist; it does not scrub the process environment.
