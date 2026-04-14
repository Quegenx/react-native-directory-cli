# rnd-cli skill

Agent skill for the [`rnd` CLI](https://github.com/Quegenx/react-native-directory-cli) — queries the React Native Directory and audits RN / Expo projects.

## Install

```bash
npx skills add Quegenx/react-native-directory-cli
```

The skill will be installed to your agent's skills directory. Works with Claude Code, Cursor, Windsurf, and any agent that supports the universal skill format.

## Prerequisites

The skill invokes the `rnd` binary. Install it separately:

```bash
cargo install rnd
```

Or build from source:

```bash
git clone https://github.com/Quegenx/react-native-directory-cli
cd react-native-directory-cli
cargo install --path .
```

Make sure `~/.cargo/bin` is on your `$PATH`.

## What this skill does

When invoked, the agent knows to:

- Query package maintenance status with `rnd info` / `rnd analyze`
- Find alternatives via `rnd alternatives` (interpreting the 4 match tiers: `[directory]` / `[topic]` / `[name]` / `[desc]`)
- Spot replacements already in the project via the `alternatives_in_project` field on each `analyze` finding
- Discover well-maintained native packages with `rnd discover`
- Filter by platform (iOS/Android/macOS/tvOS/visionOS/Windows), module type (Expo/Nitro/Turbo), Expo Go compatibility, and more

The CLI emits **facts only** — no hardcoded opinions. The agent reasons about which alternative to recommend.

## Triggers

The skill activates when you discuss:

- React Native or Expo packages
- Library maintenance status
- Alternatives / replacements for an RN library
- Auditing an RN project's dependencies
- New Architecture / Fabric / Turbo Modules / Expo Modules
- Reviewing RN code that imports unfamiliar packages

## License

MIT
