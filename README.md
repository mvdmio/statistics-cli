# statistics

`statistics` is a command-line tool for the [Statistics](https://statistics.mvdm.io) REST API. It is built for
people who script Statistics from a terminal, and for their Agents (Claude Code, Codex, Gemini CLI, and others)
that run shell commands. Every command prints JSON, and the exit code says what happened.

For the full setup steps, read the [CLI guide](https://statistics.mvdm.io/agents/cli).

The CLI talks to the Statistics REST API only. It collects no usage data and sends none: no telemetry, no
crash reports, no update checks.

## Install

Linux and macOS:

```sh
curl -LsSf https://statistics.mvdm.io/cli/install.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://statistics.mvdm.io/cli/install.ps1 | iex"
```

The installer puts the `statistics` binary in `~/.cargo/bin` (or `$CARGO_HOME/bin`) and tells you if that folder
is not on your `PATH`. Builds exist for Linux (x64 and arm64, static), macOS (arm64 and x64), and Windows (x64).

Run the install line again to upgrade: the CLI never updates itself. `statistics --version` shows the build you
run.

## For Agents

```sh
statistics skill
```

`skill` prints one Markdown skill file that teaches an Agent when and how to use the CLI: signing in, command
names, and exit codes. It is the only command whose output is not JSON.
Give it to your Agent as a skill, or let the Agent run it. The same file is [`SKILL.md`](SKILL.md) in this repo.

## Sign in

```sh
statistics login
```

`login` opens your browser at Auth. Sign in there and allow the CLI; the terminal then prints
`{"status":"logged_in","host":…,"user":{"name":…,"email":…},"account":{…}}`. While it waits, stderr holds
`{"status":"browser_opened","authorizeUrl":…}`, so you can open the link by hand. It gives up after 5 minutes.

On a machine without a browser, such as a server over SSH, use the device code. `login` falls back to it by
itself when it cannot open a browser.

```sh
statistics login --device
```

stderr then holds `{"status":"device_code","verificationUriComplete":…,"verificationUri":…,"userCode":…,"expiresIn":…}`.
Open the link on any device and sign in there. The CLI waits until you have.

The CLI finds Auth by itself, through the host's Protected Resource Metadata. You only ever set the Statistics
host.

A sign-in is an Agent connection: it acts as you, in one of your Accounts at a time. The CLI renews its access
token by itself. When the connection has ended, commands fail with `not-signed-in`; run `statistics login` again.

```sh
statistics logout
```

`logout` ends the Agent connection at Auth, so it stops working everywhere, and then forgets the sign-in. If Auth
cannot be reached, the sign-in stays, and you can run `logout` again.

### Stored sign-in

The sign-in is stored in `credentials.json`, in the `statistics` folder of your config folder:

| OS | Folder |
| -- | ------ |
| Linux | `$XDG_CONFIG_HOME/statistics` or `~/.config/statistics` |
| macOS | `~/Library/Application Support/statistics` |
| Windows | `%APPDATA%\statistics` |

`STATISTICS_CONFIG_DIR` sets another folder. The file holds one sign-in per host, so a sign-in on another host
sits beside it. On Linux and macOS, only you can read it (mode 0600).

### Personal token

For unattended jobs, such as CI, set `STATISTICS_TOKEN` to a Personal token. You create Personal tokens on the API
& Agent Access page of your account; the CLI never creates them. `STATISTICS_TOKEN` always wins over the stored
sign-in, and `logout` leaves it alone.

```sh
export STATISTICS_TOKEN=...
statistics api GET /api/v1/projects
```

## Status and Accounts

```sh
statistics status
```

`status` shows who you are and where commands land:
`{"host":…,"signedIn":…,"credential":{"kind":…,"source":…},"user":…,"account":…}`. The credential
kind is `personal-token` or `agent-connection`, and the source is `STATISTICS_TOKEN` or `stored sign-in`. `user`
comes from the sign-in, so it is `null` with a Personal token: the API has no call that names the User. Without
a credential it prints `"signedIn": false` and exits 0.

```sh
statistics accounts list
statistics accounts switch 42
```

`accounts list` lists your Accounts, the current one marked `"current": true`. `accounts switch <id>` moves the
credential into another Account, for every later command and for everyone who uses that credential.

## Commands from the API

Every REST operation is a command. The CLI builds these commands when it runs, from the host's OpenAPI description
at `/openapi/v1.json`, so a new operation shows up without a new CLI release.

```sh
statistics projects list
statistics projects create --name "Web shop"
statistics projects rename 7 --name "Shop"
statistics requests list --from 2026-10-01 --to 2026-10-07 --tag env=prod --page 2 --page-size 50
```

- **Names.** An operation's `operationId` `<group>.<action>` becomes `statistics <group> <action>`, both in
  kebab-case. So `projects.rename` is `statistics projects rename`. The group comes from the
  `operationId`, not from the tag: `statistics <group> --help` lists the tags its actions carry.
- **Arguments.** Path parameters are positional arguments, in path order. Query parameters are options, kebab-cased
  (`pageSize` is `--page-size`); lists page with `--page` and `--page-size`. The top-level fields of a
  JSON body are options too, kebab-cased and typed from the schema: numbers
  are sent as numbers, `true`/`false` as booleans, a list option repeats, and an object takes JSON text.
- **`--body <json|@file>`** sends a whole body. Field options win over the same keys in it. An operation that
  needs a body and gets none sends `{}`.
- **Name clashes.** `--help` and `--body` are always the CLI's own. After them, query
  parameters take their names before body fields. A name already taken gets a `query-` or `field-` prefix (so a
  body field `name` beside a query parameter `name` is `--field-name`), and the option's help says so.
- **Help.** `statistics --help` lists the hand-written commands and the groups. `statistics <group> --help` lists
  its actions, and `statistics <group> <action> --help` shows the operation's summary, its description, and every
  argument with its type.

The hand-written commands (`login`, `logout`, `status`, `accounts list`, `accounts switch`, `api`,
`skill`, and `--version`) never need the description, and win over a generated command of the same name. Other actions in the
same group stay reachable. A hand-written command without actions, such as `status`, hides a generated
group of the same name.

### The cached description

The CLI keeps a copy of the description per host, in the `statistics` folder of your cache folder:

| OS | Folder |
| -- | ------ |
| Linux | `$XDG_CACHE_HOME/statistics` or `~/.cache/statistics` |
| macOS | `~/Library/Caches/statistics` |
| Windows | `%LOCALAPPDATA%\statistics` |

`STATISTICS_CACHE_DIR` sets another folder. A copy younger than one hour is used as it is. An older one is
fetched again; if that fails, the old copy is used. With no copy and no answer from the host, a generated command
fails with `network`.

A new operation reaches you by itself. When you type a group or action the copy does not know, the CLI fetches a
fresh copy at once, even within the hour. If the command is still unknown, it is a usage mistake (exit 2).

## Raw calls

`statistics api <method> <path>` makes one call with your credential. The path is relative to the host, and a
query string rides in it.

```sh
statistics api GET '/api/v1/projects'
statistics api POST /api/v1/projects --body '{"name":"Web shop"}'
statistics api PATCH /api/v1/projects/7 --body @project.json
statistics api GET '/api/v1/requests?from=2026-10-01&to=2026-10-07'
```

- `--body <json|@file>` sends a JSON body, inline or read from a file.
- A success response that is not JSON fails with `binary-response`: the CLI never writes raw bytes to stdout.

## Output

- stdout holds the JSON result: pretty-printed in a terminal, compact otherwise. An empty success prints
  `{"ok":true,"status":<code>}`.
- stderr holds errors: the API's ProblemDetails as it came, or `{"error":"<code>","message":"…"}` for a failure
  on your side, such as `not-signed-in`, `network`, or `binary-response`.
- On a rate limit (429) the CLI waits for `Retry-After` and retries, up to three times.

## Exit codes

| Code | Meaning |
| ---- | ------- |
| 0 | Success. |
| 1 | Error: see stderr. |
| 2 | Usage mistake: fix the command line. |

## Environment

| Variable | Meaning |
| -------- | ------- |
| `STATISTICS_TOKEN` | A Personal token, sent as the bearer. Wins over the stored sign-in. |
| `STATISTICS_URL` | The Statistics host. Defaults to `https://statistics.mvdm.io`. |
| `STATISTICS_CONFIG_DIR` | The folder for the stored sign-in. |
| `STATISTICS_CACHE_DIR` | The folder for the cached API description. |

## Develop

The source is the [`statistics-cli`](https://github.com/mvdmio/statistics-cli) repository.

```sh
cargo build
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The tests run the built `statistics` binary against in-process fake Statistics and Auth hosts, so they need no
network, no browser, and no account. [`AGENTS.md`](AGENTS.md) holds the layout of `src/` and how to cut a release.

## Licence

MIT. See [LICENSE](LICENSE).
