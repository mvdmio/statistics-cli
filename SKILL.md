---
name: statistics
description: Use the `statistics` command-line tool to read and manage a Statistics Account (Projects, Requests, Tags, and the numbers built on them: Active Users, Throughput, and Response Time) through its REST API. Use it when a person asks you to look something up in Statistics or change a Project. Every command prints JSON.
---

# statistics

`statistics` calls the Statistics REST API at `https://statistics.mvdm.io` for the person you work for. It acts
as that person, in one of their Accounts at a time. Every command prints JSON on stdout, and the exit code says
what happened.

Use it when the person asks about or wants to change anything they keep in Statistics, such as a Project or the
Requests it received. Prefer a command over raw HTTP: the CLI signs in, builds the URL and body, and retries on
a rate limit.

Install, or upgrade, with:

```sh
curl -LsSf https://statistics.mvdm.io/cli/install.sh | sh
```

On Windows: `powershell -ExecutionPolicy ByPass -c "irm https://statistics.mvdm.io/cli/install.ps1 | iex"`.

## Sign in

Run `statistics status` first. It prints `"signedIn": true` or `false`, the credential in use, and the Account
commands act in; `user` is `null` with a Personal token. Without a credential it exits 0. With one that no longer
works it exits 1.

- `statistics login` opens the person's browser at Auth. The person signs in and allows the CLI. You wait; the
  command ends when they have.
- `statistics login --device` prints a link with the code filled in, and the code, on stderr. Show both to the
  person; they finish on any device. `login` falls back to this by itself when it cannot open a browser, such
  as over SSH.
- `STATISTICS_TOKEN` holds a Personal token for unattended runs, such as CI. It wins over the stored sign-in.
  Only the person can create one, on the API & Agent Access page of their account. Never ask them to paste one
  to you; ask them to set the variable.
- `STATISTICS_URL` points the CLI at another Statistics host. Leave it unset for `https://statistics.mvdm.io`.
- `statistics logout` ends this CLI's stored sign-in at Auth and forgets it; `STATISTICS_TOKEN` stays. Do not run
  it unless the person asks.

A command that fails with `not-signed-in` needs `statistics login`.

## Accounts

- `statistics accounts list` lists the person's Accounts. The one commands act in has `"current": true`.
- `statistics accounts switch <id>` moves the credential into another Account, for every later command. Switch
  only when the person asks for another Account, and say which one you switched to.

## Commands

Every REST operation is a command: `statistics <group> <action>`, both in kebab-case, from its `operationId`
`<group>.<action>`. The operation `projects.rename` is `statistics projects rename <id>`. The group comes from the
`operationId`, not the tag.

- `statistics --help` lists the groups. `statistics <group> --help` lists its actions.
  `statistics <group> <action> --help` shows what the operation does and every argument with its type. Read it
  before your first call of an action.
- Path parameters are positional arguments, in path order.
- Query parameters are options, kebab-cased. `requests list` takes a Date range, pages with `--page` and
  `--page-size`, and repeats `--tag`, as in
  `statistics requests list --from 2026-10-01 --to 2026-10-07 --tag env=prod --page-size 100`.
- Top-level body fields are options, kebab-cased. A list option repeats.

  ```sh
  statistics projects list
  statistics projects create --name "Web shop"
  statistics projects rename 7 --name "Shop"
  ```

- `--body '<json>'` or `--body @file.json` sends a whole body, for nested shapes. Field options win over the same
  keys in it.
- `statistics api <method> <path> [--body …]` makes one raw call, for anything the commands do not shape well:
  `statistics api GET '/api/v1/projects'`.

A group or action the CLI does not know makes it fetch the API's description again, so a new operation works at
once. A command that is still unknown is a usage mistake.

## Output and exit codes

| Code | Meaning | What to do |
| ---- | ------- | ---------- |
| 0 | Success. stdout holds the JSON result. | Read the result. |
| 1 | Error. stderr holds the API's ProblemDetails, or `{"error":"<code>","message":…}`. | Read `title`, `detail`, or `message`, fix the cause or tell the person. |
| 2 | Usage mistake. | Fix your command line; check `--help`. |

On a rate limit (429) the CLI waits and retries by itself, up to three times. Do not retry an exit 1 in a loop.

## Privacy

The CLI talks only to the Statistics host and its Auth. It sends no usage data.
