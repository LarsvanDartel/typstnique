# Typstnique

A [Typst](https://typst.app) typesetting speed game — recreate as many formulas
as you can in three minutes. Inspired by
[TeXnique](https://github.com/akshayravikumar/TeXnique) (the LaTeX version).

Full-stack **Rust**: a [Leptos](https://leptos.dev) frontend that compiles Typst
**in the browser** (via the `typst` crate compiled to WebAssembly) for instant,
zero-latency live preview, plus an [Axum](https://github.com/tokio-rs/axum) +
SQLite backend for the global leaderboard.

## How it works

- The **target** equation is rendered to SVG on load.
- You type Typst into the editor; the **preview** recompiles on every keystroke,
  entirely client-side, and is matched locally for instant feedback (the preview
  border turns green and the game auto-advances).
- The **timed** mode runs a 3-minute countdown; at the end you submit to the
  leaderboard.

### Score integrity (server-authoritative)

The leaderboard game is scored on the **server**, not in the browser:

- `start_game` creates a server-side session and returns just a token and the
  problem count. The client then fetches problems **one at a time** with
  `get_problem(session, index)` (answers included, so rendering stays
  client-side) — it never holds the whole session set at once.
- When the client detects a correct answer it calls `solve` with the typed
  answer plus keystroke telemetry (typed chars, total keydowns, backspaces,
  first-key delay, inter-keystroke timing stats). The server **re-compiles and
  matches** the answer and runs human-plausibility checks: minimum time, enough
  keystrokes to rule out paste, sub-superhuman speed, near-constant rhythm
  detection, and a **timing cross-check** — the client can't claim it spent more
  time than the server actually observed since it sent the problem.
- `finish_game` writes the session's server-tracked score to the leaderboard —
  the client never sends a score number, so forging one is impossible. Every
  accepted solve is also recorded (with its telemetry) in a `solves` table for
  later analysis.
- Names are sanitised (control chars stripped, whitespace collapsed, length
  capped). This isn't unbreakable — a determined player can forge the input
  metadata — but it removes trivial cheats (submit `i64::MAX`, paste the answer,
  script instant solves). Practice mode is purely local and isn't scored.

### Routes & logging

`/` is a landing page; the timed/leaderboard game lives at `/play` (plus
`/practice`, `/problems`, `/leaderboard`). The **server** logs with structured
[`tracing`](https://docs.rs/tracing) (`RUST_LOG` via `EnvFilter`); every server
function mints a request UUID, attaches it to its events, and embeds it in any
error so the **browser console** logs the same id — making failures traceable
end-to-end. The client keeps minimal `console_log` output.

## Project layout

```
crates/
  typst-engine/  # World impl + render_svg + answer matching (shared, compiles to WASM)
  app/           # Leptos UI components, pages, and leaderboard server functions
  frontend/      # cdylib WASM hydration entry point
  server/        # Axum binary: serves the app + leaderboard, owns the SQLite pool
assets/problems.toml   # the bundled problem set (generated from TeXnique)
assets/problems_untranslated.toml  # TeXnique problems needing manual Typst translation
migrations/            # SQLite schema
style/main.scss        # styling
```

## Editor

- **Syntax highlighting** via Typst's own parser (`typst-syntax`), rendered as a
  colored overlay behind a transparent-text textarea (pure Rust, theme-aware).
- Correct answers are auto-accepted; in **practice** mode the answer can be
  revealed. Problem order is shuffled client-side.

## Problem set

`assets/problems.toml` is the **curated, hand-checked** problem set (185
problems) and the source of truth the app loads. It was seeded from the
[TeXnique](https://github.com/akshayravikumar/TeXnique) LaTeX set — each formula
converted to Typst with [tylax](https://github.com/scipenai/tylax) and then
verified/finished by hand. The `all_problems_compile` test guarantees every
entry renders.

To pull in *new* upstream problems, run the generator:

```sh
cargo run -p typst-engine --example gen_problems
```

It **never writes `problems.toml`**. It skips everything already present (by
title) and emits only new conversions for review:

- `problems_generated.toml` — new problems that compile (review & merge by hand)
- `problems_untranslated.toml` — new problems that didn't convert (translate by hand)

…removing those files when there's nothing new. So regeneration is purely
additive and can't clobber curated edits. tylax is a **dev-dependency of
`typst-engine`**, fetched/built only for the generator — it never enters the
app/WASM build.

Points/difficulty are computed at runtime from a source-complexity heuristic
(`typst_engine::difficulty_score`), so no manual difficulty rating is stored.

## Development

Everything is provided by the Nix dev shell:

```sh
nix develop            # or: direnv allow
cargo leptos watch     # build + serve at http://127.0.0.1:3000 (hot reload)
```

Run the engine tests (native, fast):

```sh
cargo test -p typst-engine
```

Production build:

```sh
cargo leptos build --release
```

The leaderboard database is created automatically at `typstnique.db`
(`DATABASE_URL` overrides the location; it defaults in the dev shell).

## Notes / known sharp edges

- **`wasm-bindgen` version:** the `wasm-bindgen` pin in the root `Cargo.toml`
  must match `wasm-bindgen --version` from the dev shell. If cargo-leptos
  complains about a mismatch, align the pin to the CLI version.
- **WASM size:** the embedded Typst font set (`typst-assets`) makes the WASM
  bundle large. cargo-leptos runs `wasm-opt` and the server gzips responses;
  trimming to a minimal font set is a future optimisation.
- **`getrandom` on WASM:** if the build fails on `getrandom` for
  `wasm32-unknown-unknown`, add `getrandom = { version = "...", features = ["js"] }`
  to the `frontend` crate to select the browser backend.
