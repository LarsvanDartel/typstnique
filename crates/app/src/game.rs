//! The interactive game board: goal, live preview, editor, scoring, timer.
//!
//! Layout mirrors the original `TeXnique`: the goal on top, your live render
//! below, the editor at the bottom. The browser renders everything and matches
//! your answer locally for instant feedback (green border, auto-advance), but
//! in the **timed/leaderboard game the score is server-authoritative**: each
//! detected solve is sent to the server, which re-validates the answer and runs
//! plausibility checks before crediting points. Practice mode is purely local.
//!
//! [`GameBoard`] is a thin orchestrator: it owns the signals ([`GameState`]),
//! derived values, and actions, wires the effects (`wire_*`), and renders the
//! [`Hud`], [`Board`], and [`GameOverModal`] sub-components.

// `GameState` is a `Copy` bundle of reactive handles passed by value so it can
// be moved into the `'static` closures Leptos effects/components require.
#![allow(clippy::large_types_passed_by_value)]

use std::time::Duration;

use leptos::html::{Pre, Textarea};
use leptos::prelude::*;
use leptos_router::components::A;

use crate::problems::Problem;
use crate::server_fns::{
    finish_game, get_problem, solve, start_game, GameStart, InputMeta, SolveResult,
};

type StartAction = Action<(), Result<GameStart, ServerFnError>>;
type SolveAction = Action<(String, usize, String, InputMeta), Result<SolveResult, ServerFnError>>;
type FinishAction = Action<(String, String), Result<(), ServerFnError>>;

/// Per-problem keystroke log, summarised into [`InputMeta`] on solve.
#[derive(Default)]
struct KeyLog {
    /// Timestamps (ms) of each keydown.
    times: Vec<f64>,
    /// Character-producing keystrokes.
    typed: u32,
    /// Backspace/Delete presses.
    backspaces: u32,
}

/// Summarise a [`KeyLog`] (plus the problem's start time) into [`InputMeta`].
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn build_meta(k: &KeyLog, prob_start_ms: f64) -> InputMeta {
    let elapsed_ms = (js_sys::Date::now() - prob_start_ms).max(0.0) as u32;
    let keydowns = k.times.len() as u32;
    let first_key_ms = k
        .times
        .first()
        .map_or(0, |t| (t - prob_start_ms).max(0.0) as u32);

    let intervals: Vec<f64> = k.times.windows(2).map(|w| (w[1] - w[0]).max(0.0)).collect();
    let (mean, stddev, min) = if intervals.is_empty() {
        (0.0, 0.0, 0.0)
    } else {
        let n = intervals.len() as f64;
        let mean = intervals.iter().sum::<f64>() / n;
        let var = intervals.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
        let min = intervals.iter().copied().fold(f64::INFINITY, f64::min);
        (mean, var.sqrt(), min)
    };

    InputMeta {
        typed_chars: k.typed,
        elapsed_ms,
        keydowns,
        backspaces: k.backspaces,
        first_key_ms,
        mean_interval_ms: mean as u32,
        stddev_interval_ms: stddev as u32,
        min_interval_ms: if min.is_finite() { min as u32 } else { 0 },
    }
}

/// Fisher–Yates shuffle.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn shuffle(p: &mut [Problem]) {
    for i in (1..p.len()).rev() {
        let r = (js_sys::Math::random() * (i as f64 + 1.0)) as usize;
        p.swap(i, r.min(i));
    }
}

/// All reactive state for one game board. Every field is `Copy`, so the struct
/// is `Copy` and can be handed to sub-components and effect helpers by value.
#[derive(Clone, Copy)]
struct GameState {
    /// The bundled problems (practice), kept to (re)shuffle on the client.
    seed: StoredValue<Vec<Problem>>,
    /// The active problem set (practice). Server mode uses `total` + fetches.
    problems: RwSignal<Vec<Problem>>,
    total: RwSignal<usize>,
    session: RwSignal<Option<String>>,
    index: RwSignal<usize>,
    source: RwSignal<String>,
    score: RwSignal<u32>,
    solved: RwSignal<u32>,
    time_left: RwSignal<i32>,
    /// The game has ended (timer hit zero). Stays true until a restart, so the
    /// editor stays disabled even while the summary popup is dismissed.
    game_over: RwSignal<bool>,
    /// Whether the end-of-game summary popup is currently shown. Toggled
    /// independently of `game_over` so closing it doesn't restart the game.
    summary_open: RwSignal<bool>,
    name: RwSignal<String>,
    show_answer: RwSignal<bool>,
    submitted: RwSignal<bool>,
    submit_err: RwSignal<Option<String>>,
    /// Why the last solve attempt wasn't counted (server rejection or network
    /// error), shown inline by the editor until the next problem.
    reject_notice: RwSignal<Option<String>>,
    /// Set when starting a game or fetching a problem fails, so the board can
    /// offer a retry instead of spinning on "Loading…".
    load_err: RwSignal<Option<String>>,
    keys: StoredValue<KeyLog>,
    prob_start_ms: RwSignal<f64>,
    timed: bool,
    server_scored: bool,
}

impl GameState {
    fn new(timed: bool, server_scored: bool, seed: Vec<Problem>) -> Self {
        Self {
            seed: StoredValue::new(seed),
            problems: RwSignal::new(Vec::new()),
            total: RwSignal::new(0),
            session: RwSignal::new(None),
            index: RwSignal::new(0),
            source: RwSignal::new(String::new()),
            score: RwSignal::new(0),
            solved: RwSignal::new(0),
            time_left: RwSignal::new(if timed { 180 } else { 0 }),
            game_over: RwSignal::new(false),
            summary_open: RwSignal::new(false),
            name: RwSignal::new(String::new()),
            show_answer: RwSignal::new(false),
            submitted: RwSignal::new(false),
            submit_err: RwSignal::new(None),
            reject_notice: RwSignal::new(None),
            load_err: RwSignal::new(None),
            keys: StoredValue::new(KeyLog::default()),
            prob_start_ms: RwSignal::new(0.0),
            timed,
            server_scored,
        }
    }
}

// ── effect wiring ───────────────────────────────────────────────────────────

/// Start a server session (and reinstall on restart) or, in practice, load and
/// shuffle the bundled problems once.
fn wire_session(state: GameState, start: StartAction, ta_ref: NodeRef<Textarea>) {
    if state.server_scored {
        Effect::new(move |ran: Option<()>| {
            if ran.is_none() {
                start.dispatch(());
            }
        });
        Effect::new(move |_| match start.value().get() {
            Some(Ok(game)) => {
                log::info!(
                    "game started: session={} total={}",
                    game.session,
                    game.total
                );
                state.session.set(Some(game.session));
                state.total.set(game.total);
                state.index.set(0);
                state.score.set(0);
                state.solved.set(0);
                state.time_left.set(180);
                state.source.set(String::new());
                state.submitted.set(false);
                state.submit_err.set(None);
                state.game_over.set(false);
                state.summary_open.set(false);
                state.load_err.set(None);
                if let Some(ta) = ta_ref.get_untracked() {
                    ta.set_value("");
                }
            },
            Some(Err(e)) => {
                log::error!("start_game failed: {e}");
                state.load_err.set(Some("Couldn't start the game.".into()));
            },
            None => {},
        });
    } else {
        Effect::new(move |ran: Option<()>| {
            if ran.is_none() {
                let mut p = state.seed.get_value();
                shuffle(&mut p);
                state.problems.set(p);
            }
        });
    }
}

/// Clear the editor and move to the next problem after a short beat (so the
/// "correct" highlight is visible before advancing).
fn advance_after_beat(state: GameState) {
    set_timeout(
        move || {
            state.source.set(String::new());
            state.index.update(|i| *i += 1);
        },
        Duration::from_millis(550),
    );
}

/// Reflect server results: the authoritative score, and whether to advance.
/// In the server game the advance happens *here* (only on acceptance) rather
/// than optimistically, so a rejected solve keeps the player on the problem and
/// shows why, and there's no race between crediting and fetching the next one.
fn wire_result_effects(state: GameState, solve_action: SolveAction, finish: FinishAction) {
    Effect::new(move |_| match solve_action.value().get() {
        Some(Ok(r)) => {
            state.score.set(r.score);
            state.solved.set(r.solved);
            if r.accepted {
                log::debug!("solve accepted: score={} solved={}", r.score, r.solved);
                state.reject_notice.set(None);
                advance_after_beat(state);
            } else {
                let reason = r.reason.unwrap_or_else(|| "not counted".into());
                log::warn!(
                    "solve rejected by server: {reason} (request_id={})",
                    r.request_id
                );
                state.reject_notice.set(Some(reason));
            }
        },
        Some(Err(e)) => {
            log::error!("solve failed: {e}");
            state
                .reject_notice
                .set(Some("Network error — solve not counted.".into()));
        },
        None => {},
    });

    Effect::new(move |_| {
        if let Some(Err(e)) = finish.value().get() {
            log::error!("finish_game failed: {e}");
            state.submit_err.set(Some(e.to_string()));
            state.submitted.set(false);
        }
    });
}

/// Reset per-problem state (editor, keystroke log, timer) whenever the problem changes.
fn wire_reset(state: GameState, ta_ref: NodeRef<Textarea>) {
    Effect::new(move |_| {
        state.index.track();
        if let Some(ta) = ta_ref.get_untracked() {
            ta.set_value("");
        }
        state.show_answer.set(false);
        state.reject_notice.set(None);
        state.load_err.set(None);
        state.keys.set_value(KeyLog::default());
        state.prob_start_ms.set(js_sys::Date::now());
    });
}

/// On first detected correctness, credit points (server in the leaderboard game,
/// locally in practice) and advance after a short beat.
fn wire_auto_accept(
    state: GameState,
    problem: Signal<Option<Problem>>,
    is_correct: Memo<bool>,
    solve_action: SolveAction,
) {
    Effect::new(move |prev: Option<bool>| {
        let correct = is_correct.get();
        if correct && prev != Some(true) && !state.game_over.get_untracked() {
            if let Some(p) = problem.get_untracked() {
                if let Some(sess) = state.session.get_untracked() {
                    // Server path: dispatch solve; the advance fires only on
                    // acceptance inside wire_result_effects, avoiding the race
                    // where get_problem(new) sets current before solve(old) lands.
                    let meta = state
                        .keys
                        .with_value(|k| build_meta(k, state.prob_start_ms.get_untracked()));
                    let n = state.total.get_untracked().max(1);
                    solve_action.dispatch((
                        sess,
                        state.index.get_untracked() % n,
                        state.source.get_untracked(),
                        meta,
                    ));
                } else {
                    // Practice path: credit locally and advance after the green beat.
                    state.score.update(|s| *s += p.points());
                    state.solved.update(|s| *s += 1);
                    advance_after_beat(state);
                }
            }
        }
        correct
    });
}

/// The 3-minute countdown (timed mode only).
fn wire_timer(state: GameState) {
    if !state.timed {
        return;
    }
    Effect::new(move |_| {
        let handle = set_interval_with_handle(
            move || {
                let t = state.time_left.get_untracked();
                if t <= 0 {
                    return;
                }
                if t <= 1 {
                    state.time_left.set(0);
                    state.game_over.set(true);
                    state.summary_open.set(true);
                } else {
                    state.time_left.set(t - 1);
                }
            },
            Duration::from_secs(1),
        );
        if let Ok(handle) = handle {
            on_cleanup(move || handle.clear());
        }
    });
}

// ── sub-components ──────────────────────────────────────────────────────────

/// Problem title/difficulty + timer/score/solved stats.
#[component]
fn Hud(state: GameState, problem: Signal<Option<Problem>>) -> impl IntoView {
    let timer_label = move || {
        let t = state.time_left.get().max(0);
        format!("{}:{:02}", t / 60, t % 60)
    };
    view! {
        <div class="hud">
            <div class="problem-meta">
                <div class="problem-title">{move || problem.get().map(|p| p.title)}</div>
                <div class="difficulty">
                    {move || problem.get().map(|p| "★".repeat(usize::try_from(p.difficulty()).unwrap_or(0)))}
                    " · " {move || problem.get().map(|p| p.points())} " pts"
                </div>
            </div>
            <div class="hud-stats">
                {move || state.timed.then(|| view! {
                    <div class="stat timer" class:low=move || state.time_left.get() <= 30>
                        <span class="label">"Time"</span>
                        <b>{timer_label}</b>
                    </div>
                })}
                <div class="stat"><span class="label">"Score"</span> <b>{move || state.score.get()}</b></div>
                <div class="stat"><span class="label">"Solved"</span> <b>{move || state.solved.get()}</b></div>
            </div>
        </div>
    }
}

/// Goal, live preview, and editor panels.
#[component]
fn Board(
    state: GameState,
    problem: Signal<Option<Problem>>,
    target_svg: Memo<String>,
    preview: Signal<String>,
    highlighted: Signal<String>,
    is_correct: Memo<bool>,
    ta_ref: NodeRef<Textarea>,
    hl_ref: NodeRef<Pre>,
    solve_action: SolveAction,
    load_refetch: RwSignal<u32>,
) -> impl IntoView {
    let on_skip = move |_| {
        state.source.set(String::new());
        state.index.update(|i| *i += 1);
    };
    let on_input = move |_| {
        if let Some(ta) = ta_ref.get_untracked() {
            state.source.set(ta.value());
        }
    };
    // Keystroke telemetry (timing, typed chars, backspaces) for plausibility.
    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        let key = ev.key();
        state.keys.update_value(|k| {
            k.times.push(js_sys::Date::now());
            if key == "Backspace" || key == "Delete" {
                k.backspaces += 1;
            } else if key.chars().count() == 1 {
                k.typed += 1;
            }
        });
    };
    let on_scroll = move |_| {
        if let (Some(ta), Some(pre)) = (ta_ref.get_untracked(), hl_ref.get()) {
            pre.set_scroll_top(ta.scroll_top());
            pre.set_scroll_left(ta.scroll_left());
        }
    };

    view! {
        <div class="game">
            <div class="panel">
                <h3>"Goal"</h3>
                {move || match (state.load_err.get(), problem.get().is_some()) {
                    (Some(err), false) => view! {
                        <p class="diag">{err}</p>
                        <button class="ghost" on:click=move |_| load_refetch.update(|n| *n += 1)>
                            "Retry"
                        </button>
                    }.into_any(),
                    _ => view! {
                        <div class="target" inner_html=move || target_svg.get()></div>
                    }.into_any(),
                }}
            </div>

            <div class="panel">
                <h3>"Your render"</h3>
                <div class="preview" class:correct=move || is_correct.get() inner_html=move || preview.get()></div>
            </div>

            <div class="panel editor-panel">
                <h3>"Type Typst"</h3>
                <div class="editor-wrap">
                    <pre class="hl" node_ref=hl_ref aria-hidden="true" inner_html=move || highlighted.get()></pre>
                    <textarea
                        class="editor"
                        node_ref=ta_ref
                        spellcheck="false"
                        autocomplete="off"
                        prop:disabled=move || state.game_over.get()
                        on:input=on_input
                        on:keydown=on_keydown
                        on:scroll=on_scroll
                        placeholder="Type Typst here…"
                    ></textarea>
                </div>
                <div class="actions">
                    <button class="ghost" prop:disabled=move || state.game_over.get() on:click=on_skip>"Skip"</button>
                    {(!state.timed).then(|| view! {
                        <button class="ghost" on:click=move |_| state.show_answer.update(|s| *s = !*s)>
                            {move || if state.show_answer.get() { "Hide answer" } else { "Show answer" }}
                        </button>
                    })}
                    <span class="hint">"Correct answers are accepted automatically."</span>
                </div>
                {move || state.reject_notice.get().map(|reason| view! {
                    <div class="solve-notice">
                        <span class="solve-notice-reason">{reason}</span>
                        <button class="ghost" on:click=move |_| {
                            state.reject_notice.set(None);
                            if let Some(sess) = state.session.get_untracked() {
                                let meta = state.keys.with_value(|k| build_meta(k, state.prob_start_ms.get_untracked()));
                                let n = state.total.get_untracked().max(1);
                                solve_action.dispatch((
                                    sess,
                                    state.index.get_untracked() % n,
                                    state.source.get_untracked(),
                                    meta,
                                ));
                            }
                        }>"Try again"</button>
                        <button class="ghost" on:click=move |_| state.reject_notice.set(None)>"×"</button>
                    </div>
                })}
                {move || (!state.timed && state.show_answer.get()).then(|| view! {
                    <div class="answer">
                        <h3>"Answer"</h3>
                        <pre class="answer-src">{move || problem.get().map(|p| p.source)}</pre>
                    </div>
                })}
            </div>
        </div>
    }
}

/// End-of-game modal: submit name to the leaderboard, or play again.
#[component]
fn GameOverModal(state: GameState, start: StartAction, finish: FinishAction) -> impl IntoView {
    let on_submit_score = move |_| {
        if state.submitted.get_untracked() || finish.pending().get_untracked() {
            return;
        }
        let Some(sess) = state.session.get_untracked() else {
            return;
        };
        state.submitted.set(true);
        state.submit_err.set(None);
        finish.dispatch((sess, state.name.get_untracked()));
    };

    let play_again = move |_| {
        state.submitted.set(false);
        state.submit_err.set(None);
        state.summary_open.set(false);
        if state.server_scored {
            state.game_over.set(false);
            start.dispatch(()); // fresh session; install effect resets the rest
        } else {
            // (Practice has no timer, so this modal never opens there — kept for
            // completeness. The reset effect clears the editor on index change.)
            state.score.set(0);
            state.solved.set(0);
            state.index.set(0);
            state.source.set(String::new());
            state.time_left.set(if state.timed { 180 } else { 0 });
            state.game_over.set(false);
            state.problems.update(|p| shuffle(p));
        }
    };

    view! {
        // The summary popup. Closing it only hides the popup — the game stays
        // over (editor disabled) and the bar below offers a way back.
        {move || state.summary_open.get().then(|| view! {
            <div class="modal">
                <div class="modal-box">
                    <button class="modal-close" title="Close" on:click=move |_| state.summary_open.set(false)>"×"</button>
                    <h2>"Time's up!"</h2>
                    <p>
                        "You scored " <b>{move || state.score.get()}</b>
                        " points (" {move || state.solved.get()} " solved)."
                    </p>
                    {move || if state.submitted.get() {
                        view! {
                            <p class="submitted">"✓ Score submitted"</p>
                            <p><A href="/leaderboard">"View leaderboard →"</A></p>
                        }.into_any()
                    } else {
                        view! {
                            <input
                                placeholder="Your name"
                                prop:value=move || state.name.get()
                                on:input=move |ev| state.name.set(event_target_value(&ev))
                            />
                            <button
                                prop:disabled=move || finish.pending().get()
                                on:click=on_submit_score
                            >"Submit score"</button>
                            {move || state.submit_err.get().map(|e| view! { <p class="diag">{e}</p> })}
                        }.into_any()
                    }}
                    <div class="modal-actions">
                        <button class="ghost" on:click=play_again>"Play again"</button>
                        <A href="/leaderboard">"Leaderboard"</A>
                    </div>
                </div>
            </div>
        })}
        // Persistent bar shown once the game is over and the popup is dismissed,
        // so the player is never stranded without a way to restart.
        {move || (state.game_over.get() && !state.summary_open.get()).then(|| view! {
            <div class="game-over-bar">
                <span class="game-over-label">"Game over · " {move || state.score.get()} " pts"</span>
                <button class="ghost" on:click=move |_| state.summary_open.set(true)>"Summary"</button>
                <button on:click=play_again>"Play again"</button>
            </div>
        })}
    }
}

// ── orchestrator ────────────────────────────────────────────────────────────

/// The playable board.
///
/// * `timed` — run a 3-minute countdown and the game-over flow.
/// * `server_scored` — fetch problems from the server and score authoritatively (the leaderboard
///   game). When false, problems come from the `problems` prop and scoring is local (practice).
#[allow(clippy::too_many_lines)]
#[component]
pub fn GameBoard(
    #[prop(optional)] problems: Vec<Problem>,
    #[prop(optional)] timed: bool,
    #[prop(optional)] server_scored: bool,
) -> impl IntoView {
    // Both modes start empty and populate on the client (server: fetched per
    // index; practice: the shuffled bundle), so SSR never shows an unshuffled
    // problem that then changes on hydration.
    let state = GameState::new(timed, server_scored, problems);
    let ta_ref: NodeRef<Textarea> = NodeRef::new();
    let hl_ref: NodeRef<Pre> = NodeRef::new();

    // Fetch the active problem from the server, one at a time. The session is
    // created client-side, so this is a `LocalResource` (client-only): it loads
    // after hydration and refetches when the signals it reads change.
    let load_refetch = RwSignal::new(0u32);
    let problem_res = LocalResource::new(move || {
        let sess = state.session.get();
        let idx = state.index.get();
        let n = state.total.get();
        load_refetch.get();
        async move {
            match sess {
                Some(s) if n > 0 => match get_problem(s, idx % n).await {
                    Ok(p) => {
                        state.load_err.set(None);
                        Some(p)
                    },
                    Err(e) => {
                        log::error!("get_problem failed: {e}");
                        state.load_err.set(Some("Couldn't load problem.".into()));
                        None
                    },
                },
                _ => None,
            }
        }
    });

    let problem = Signal::derive(move || {
        if state.server_scored {
            problem_res.get().flatten()
        } else {
            state.problems.with(|p| {
                let n = p.len();
                (n > 0).then(|| p[state.index.get() % n].clone())
            })
        }
    });
    let target_svg = Memo::new(move |_| {
        problem.get().map_or_else(String::new, |p| {
            typst_engine::render_svg(&p.source, p.kind).unwrap_or_default()
        })
    });
    let preview = Signal::derive(move || match problem.get() {
        Some(p) => match typst_engine::render_svg(&state.source.get(), p.kind) {
            Ok(svg) => svg,
            Err(diag) => format!("<pre class=\"diag\">{diag}</pre>"),
        },
        None => String::new(),
    });
    let highlighted = Signal::derive(move || {
        problem.get().map_or_else(String::new, |p| {
            typst_engine::highlight_html(&state.source.get(), p.kind)
        })
    });
    let is_correct = Memo::new(move |_| match problem.get() {
        Some(p) => {
            let src = state.source.get();
            !src.trim().is_empty() && typst_engine::matches(&p.source, &src, p.kind)
        },
        None => false,
    });

    let start_action: StartAction = Action::new(|(): &()| async move { start_game().await });
    let solve_action: SolveAction = Action::new(|input: &(String, usize, String, InputMeta)| {
        let (sess, idx, ans, meta) = input.clone();
        async move { solve(sess, idx, ans, meta).await }
    });
    let finish_action: FinishAction = Action::new(|input: &(String, String)| {
        let (sess, name) = input.clone();
        async move { finish_game(sess, name).await }
    });

    wire_session(state, start_action, ta_ref);
    wire_result_effects(state, solve_action, finish_action);
    wire_reset(state, ta_ref);
    wire_auto_accept(state, problem, is_correct, solve_action);
    wire_timer(state);

    let count = move || {
        if state.server_scored {
            state.total.get()
        } else {
            state.problems.with(Vec::len)
        }
    };

    view! {
        {move || if count() == 0 {
            if let Some(err) = state.load_err.get() {
                view! {
                    <p class="diag">{err}</p>
                    <button on:click=move |_| { start_action.dispatch(()); }>"Retry"</button>
                }.into_any()
            } else {
                view! { <p class="hint">"Loading…"</p> }.into_any()
            }
        } else {
            view! {
                <Hud state=state problem=problem/>
                <Board
                    state=state
                    problem=problem
                    target_svg=target_svg
                    preview=preview
                    highlighted=highlighted
                    is_correct=is_correct
                    ta_ref=ta_ref
                    hl_ref=hl_ref
                    solve_action=solve_action
                    load_refetch=load_refetch
                />
            }.into_any()
        }}
        <GameOverModal state=state start=start_action finish=finish_action/>
    }
}
