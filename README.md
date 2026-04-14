# rnd — React Native Directory CLI

Query [reactnative.directory](https://reactnative.directory) and audit React Native projects from the terminal. Designed for AI agents (Claude Code, Codex) and humans alike.

```
$ rnd alternatives @react-native-async-storage/async-storage --limit 3
Candidates matching @react-native-async-storage/async-storage:

[topic] expo-sqlite                ★48.7k   241.9k/wk   [ios,and]
[topic] react-native-mmkv-storage  ★1.7k    10.3k/wk    [ios,and]
[topic] realm                      ★6.0k    44.5k/wk    [ios,and]
```

## What it does

- **Search** 2,400+ React Native and Expo packages
- **Audit** a project's `package.json` + source files for unmaintained / archived / missing-new-arch deps
- **Find alternatives** with a 4-tier algorithm (directory-curated → topics → name tokens → description keywords)
- **Discover** quietly-great packages that don't dominate download charts
- **Facts only, no opinions** — the CLI emits ground truth; the consuming agent (or you) reasons about it

## Install

```bash
cargo install --path .
```

Add `~/.cargo/bin` to your `$PATH` if it isn't already:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

Prebuilt binaries via `cargo-dist` / GitHub Releases coming soon.

## Commands

| Command | Purpose |
|---|---|
| `rnd search <query>` | Find packages by name/description/topic |
| `rnd info <pkg>` | Detailed facts on one package |
| `rnd alternatives <pkg>` | Candidate replacements, tagged by how they matched |
| `rnd trending` | Top packages by weekly downloads (native-only by default) |
| `rnd discover` | Well-rated, actively-maintained native packages |
| `rnd list [--category X --platform Y --new-arch]` | Filter packages |
| `rnd compare <a> <b>` | Side-by-side comparison |
| `rnd analyze [path]` | Scan a project's `package.json` + source for facts |
| `rnd cache [info\|clear]` | Manage the 24h-TTL cache |

Global flags: `--json` (force JSON) · `--pretty` (force pretty) · `--refresh` (bypass cache) · `--quiet` (suppress non-essential output)

## Auto-JSON for AI agents

`rnd` detects `CLAUDECODE`, `CODEX`, and `CODEX_CI` environment variables and auto-emits JSON. Pipe the output through `jq` or parse it directly — no extra flags needed.

```bash
# Inside Claude Code — returns JSON automatically
rnd analyze . | jq '.package_findings[] | select(.flags | index("unmaintained"))'
```

## `rnd analyze` — the killer feature

Point it at any React Native project:

```
$ rnd analyze .
Scanned 335 source files, 58 deps, 4 flagged

⚠ react-native-easing-gradient — used in 1 file
    - unmaintained
    already in deps that could cover this: expo-linear-gradient
    repo: https://github.com/phamfoo/react-native-easing-gradient
    stats: ★258  12.6k/wk  score 0  pushed 2024-02-27
    src/components/shell/layout/.../animated-header-panel-scroll-view.tsx:11
```

For each flagged dep it emits:

- **flags** — `archived`, `unmaintained`, `no new-arch support`, `no TypeScript types`
- **alternatives_in_project** — packages already in your `package.json` that could replace the flagged one (the highest-value signal for agents)
- **github_url**, **license**, **last_pushed_at** — activity data
- **used_in** — exact `file:line` locations where the dep is imported

## `rnd alternatives` — 4-tier matching

Candidates are tagged by how they matched:

| Tag | Source | Signal |
|---|---|---|
| `[directory]` | The directory API's own curated `alternatives[]` field | Strongest — maintainer-curated |
| `[topic]` | GitHub topic Jaccard overlap, after stopword filter | Strong for well-tagged libs |
| `[name]` | Package-name token overlap | Medium — catches niche libs |
| `[desc]` | Description keyword overlap (with stemming) | Weakest — loosely related |

Hard filters always apply:
- If target has native code, candidates must too
- If target is pure-JS, native alternatives are allowed (you might wrap them yourself)
- Archived/unmaintained candidates are excluded

## Design principle

The CLI emits **facts only**:

- Directory metadata (stars, downloads, topics, score, license, activity dates)
- Flags from the directory's own maintenance data
- File:line locations of imports in your codebase

It does **not**:

- Hardcode opinions ("use MMKV instead of AsyncStorage")
- Compute arbitrary scores with made-up weights
- Suggest swaps based on training data that rots

Agents (and humans) reason better than any hardcoded registry. The CLI gives you ground truth; you decide what to do.

## AI agent skill

A [Claude Code skill](https://github.com/anthropics/claude-code) ships in `.claude/skills/rnd-cli/`. It triggers on RN/Expo package discussions, audit requests, "is X maintained?" questions, and migration planning. Agents auto-reach for `rnd` without explicit prompting.

## Built with

- [clap](https://github.com/clap-rs/clap) 4 — CLI framework
- [reqwest](https://github.com/seanmonstar/reqwest) + [tokio](https://tokio.rs) — API client
- [regex](https://github.com/rust-lang/regex) + [walkdir](https://github.com/BurntSushi/walkdir) — source scanning
- [owo-colors](https://github.com/jam1garner/owo-colors) + [indicatif](https://github.com/console-rs/indicatif) — pretty output

## Cache

The directory dump (~2.4 MB, 2,419 packages) is cached for 24 hours at your OS cache location:

- macOS: `~/Library/Caches/dev.rnd.rnd/libraries.json`
- Linux: `~/.cache/rnd/libraries.json`

Bypass with `--refresh`, inspect with `rnd cache info`, clear with `rnd cache clear`.

## License

MIT.
