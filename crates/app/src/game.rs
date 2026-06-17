//! The interactive game board: goal, live preview, editor, scoring, timer.
//!
//! Layout mirrors the original `TeXnique`: the goal on top, your live render
//! below, the editor at the bottom. The browser renders everything and matches
//! your answer locally for instant feedback (green border, auto-advance), but
//! in the **timed/leaderboard game the score is server-authoritative**: each
//! detected solve is sent to the server, which re-validates the answer and runs
//! plausibility checks before crediting points. Practice mode is purely local.

use std::time::Duration;

use leptos::html::{Pre, Textarea};
use leptos::prelude::*;
use leptos_router::components::A;

use crate::problems::Problem;
use crate::server_fns::{finish_game, get_problem, solve, start_game, InputMeta};

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
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
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

/// The playable board.
///
/// * `timed` — run a 3-minute countdown and the game-over flow.
/// * `server_scored` — fetch problems from the server and score authoritatively
///   (the leaderboard game). When false, problems come from the `problems` prop
///   and scoring is local (practice).
#[component]
#[allow(clippy::too_many_lines)]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn GameBoard(
    #[prop(optional)] problems: Vec<Problem>,
    #[prop(optional)] timed: bool,
    #[prop(optional)] server_scored: bool,
) -> impl IntoView {
    // Both modes start empty and populate on the client (server: fetched per
    // index; practice: the shuffled bundle), so SSR never shows an unshuffled
    // problem that then changes on hydration.
    let seed = StoredValue::new(problems);
    let problems = RwSignal::new(Vec::<Problem>::new());
    // Server mode fetches problems one at a time; `total` is the session size.
    let total = RwSignal::new(0usize);
    let session = RwSignal::new(None::<String>);
    let index = RwSignal::new(0usize);
    let source = RwSignal::new(String::new());
    let score = RwSignal::new(0u32);
    let solved = RwSignal::new(0u32);
    let time_left = RwSignal::new(if timed { 180i32 } else { 0 });
    let game_over = RwSignal::new(false);
    let name = RwSignal::new(String::new());
    let show_answer = RwSignal::new(false);
    let submitted = RwSignal::new(false);
    let submit_err = RwSignal::new(Option::<String>::None);
    let keys = StoredValue::new(KeyLog::default());
    let prob_start_ms = RwSignal::new(0f64);

    let ta_ref: NodeRef<Textarea> = NodeRef::new();
    let hl_ref: NodeRef<Pre> = NodeRef::new();

    // Fetch the active problem from the server, one at a time. The session is
    // created client-side, so this is a `LocalResource` (client-only): it loads
    // after hydration and refetches when the signals it reads change — no manual
    // dispatch, so no request loop.
    let problem_res = LocalResource::new(move || {
        let sess = session.get();
        let idx = index.get();
        let n = total.get();
        async move {
            match sess {
                Some(s) if n > 0 => {
                    log::debug!("fetching problem {idx} of {n}");
                    match get_problem(s, idx % n).await {
                        Ok(p) => Some(p),
                        Err(e) => {
                            log::error!("get_problem failed: {e}");
                            None
                        }
                    }
                }
                _ => None,
            }
        }
    });

    let count = move || {
        if server_scored {
            total.get()
        } else {
            problems.with(Vec::len)
        }
    };
    let problem = move || {
        if server_scored {
            problem_res.get().flatten()
        } else {
            problems.with(|p| {
                let n = p.len();
                (n > 0).then(|| p[index.get() % n].clone())
            })
        }
    };

    let shuffle = |p: &mut Vec<Problem>| {
        for i in (1..p.len()).rev() {
            let r = (js_sys::Math::random() * (i as f64 + 1.0)) as usize;
            p.swap(i, r.min(i));
        }
    };

    let start_action = Action::new(|(): &()| async move { start_game().await });
    let solve_action = Action::new(|input: &(String, usize, String, InputMeta)| {
        let (sess, idx, ans, meta) = input.clone();
        async move { solve(sess, idx, ans, meta).await }
    });
    let finish_action = Action::new(|input: &(String, String)| {
        let (sess, name) = input.clone();
        async move { finish_game(sess, name).await }
    });

    if server_scored {
        // Begin a server session on mount.
        Effect::new(move |ran: Option<()>| {
            if ran.is_none() {
                start_action.dispatch(());
            }
        });
        // Install the session and (re)start the round; `problem_res` refetches
        // automatically when session/index change.
        Effect::new(move |_| match start_action.value().get() {
            Some(Ok(game)) => {
                log::info!("game started: session={} total={}", game.session, game.total);
                session.set(Some(game.session));
                total.set(game.total);
                index.set(0);
                score.set(0);
                solved.set(0);
                time_left.set(180);
                source.set(String::new());
                submitted.set(false);
                submit_err.set(None);
                game_over.set(false);
                if let Some(ta) = ta_ref.get_untracked() {
                    ta.set_value("");
                }
            }
            Some(Err(e)) => log::error!("start_game failed: {e}"),
            None => {}
        });
    } else {
        // Practice: load and shuffle the bundled problems once, client-side.
        Effect::new(move |ran: Option<()>| {
            if ran.is_none() {
                let mut p = seed.get_value();
                shuffle(&mut p);
                problems.set(p);
            }
        });
    }

    // Server score is authoritative: reflect it whenever a solve resolves.
    Effect::new(move |_| match solve_action.value().get() {
        Some(Ok(r)) => {
            log::debug!(
                "solve {}: score={} solved={}",
                if r.accepted { "accepted" } else { "rejected" },
                r.score,
                r.solved
            );
            score.set(r.score);
            solved.set(r.solved);
        }
        Some(Err(e)) => log::error!("solve failed: {e}"),
        None => {}
    });

    // Reset per-problem state (editor, keystroke log, timer) on change.
    Effect::new(move |_| {
        index.track();
        if let Some(ta) = ta_ref.get_untracked() {
            ta.set_value("");
        }
        show_answer.set(false);
        keys.set_value(KeyLog::default());
        prob_start_ms.set(js_sys::Date::now());
    });

    let target_svg = Memo::new(move |_| {
        problem().map_or_else(String::new, |p| {
            typst_engine::render_svg(&p.source, p.kind).unwrap_or_default()
        })
    });

    let preview = move || {
        let Some(p) = problem() else {
            return String::new();
        };
        match typst_engine::render_svg(&source.get(), p.kind) {
            Ok(svg) => svg,
            Err(diag) => format!("<pre class=\"diag\">{diag}</pre>"),
        }
    };

    let highlighted = move || {
        problem().map_or_else(String::new, |p| typst_engine::highlight_html(&source.get(), p.kind))
    };

    let is_correct = Memo::new(move |_| {
        let Some(p) = problem() else {
            return false;
        };
        let src = source.get();
        !src.trim().is_empty() && typst_engine::matches(&p.source, &src, p.kind)
    });

    // Auto-accept: on first detected correctness, credit points (server in the
    // leaderboard game, locally in practice) and advance after a short beat.
    Effect::new(move |prev: Option<bool>| {
        let correct = is_correct.get();
        if correct && prev != Some(true) && !game_over.get_untracked() {
            if let Some(p) = problem() {
                if let Some(sess) = session.get_untracked() {
                    let meta = keys.with_value(|k| build_meta(k, prob_start_ms.get_untracked()));
                    let n = total.get_untracked().max(1);
                    solve_action.dispatch((
                        sess,
                        index.get_untracked() % n,
                        source.get_untracked(),
                        meta,
                    ));
                } else {
                    score.update(|s| *s += p.points());
                    solved.update(|s| *s += 1);
                }
            }
            set_timeout(
                move || {
                    source.set(String::new());
                    index.update(|i| *i += 1);
                },
                Duration::from_millis(550),
            );
        }
        correct
    });

    if timed {
        Effect::new(move |_| {
            let handle = set_interval_with_handle(
                move || {
                    let t = time_left.get_untracked();
                    if t <= 0 {
                        return;
                    }
                    if t <= 1 {
                        time_left.set(0);
                        game_over.set(true);
                    } else {
                        time_left.set(t - 1);
                    }
                },
                Duration::from_secs(1),
            );
            if let Ok(handle) = handle {
                on_cleanup(move || handle.clear());
            }
        });
    }

    Effect::new(move |_| {
        if let Some(Err(e)) = finish_action.value().get() {
            log::error!("finish_game failed: {e}");
            submit_err.set(Some(e.to_string()));
            submitted.set(false);
        }
    });

    let timer_label = move || {
        let t = time_left.get().max(0);
        format!("{}:{:02}", t / 60, t % 60)
    };

    let on_skip = move |_| {
        source.set(String::new());
        index.update(|i| *i += 1);
    };

    let on_input = move |_| {
        if let Some(ta) = ta_ref.get_untracked() {
            source.set(ta.value());
        }
    };

    // Record keystroke telemetry (timing, typed chars, backspaces) for plausibility.
    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        let key = ev.key();
        keys.update_value(|k| {
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

    let on_submit_score = move |_| {
        if submitted.get_untracked() || finish_action.pending().get_untracked() {
            return;
        }
        let Some(sess) = session.get_untracked() else {
            return;
        };
        submitted.set(true);
        submit_err.set(None);
        finish_action.dispatch((sess, name.get_untracked()));
    };

    let play_again = move |_| {
        submitted.set(false);
        submit_err.set(None);
        if server_scored {
            game_over.set(false);
            start_action.dispatch(()); // fresh session; install effect resets the rest
        } else {
            score.set(0);
            solved.set(0);
            index.set(0);
            source.set(String::new());
            time_left.set(if timed { 180 } else { 0 });
            game_over.set(false);
            problems.update(|p| shuffle(p));
            if let Some(ta) = ta_ref.get_untracked() {
                ta.set_value("");
            }
        }
    };

    view! {
        {move || if count() == 0 {
            view! { <p class="hint">"Loading…"</p> }.into_any()
        } else {
            view! {
                <div class="hud">
                    <div class="problem-meta">
                        <div class="problem-title">{move || problem().map(|p| p.title)}</div>
                        <div class="difficulty">
                            {move || problem().map(|p| "★".repeat(p.difficulty() as usize))}
                            " · " {move || problem().map(|p| p.points())} " pts"
                        </div>
                    </div>
                    <div class="hud-stats">
                        {move || timed.then(|| view! {
                            <div class="stat timer" class:low=move || time_left.get() <= 30>
                                <span class="label">"Time"</span>
                                <b>{timer_label}</b>
                            </div>
                        })}
                        <div class="stat"><span class="label">"Score"</span> <b>{move || score.get()}</b></div>
                        <div class="stat"><span class="label">"Solved"</span> <b>{move || solved.get()}</b></div>
                    </div>
                </div>

                <div class="game">
                    <div class="panel">
                        <h3>"Goal"</h3>
                        <div class="target" inner_html=move || target_svg.get()></div>
                    </div>

                    <div class="panel">
                        <h3>"Your render"</h3>
                        <div class="preview" class:correct=move || is_correct.get() inner_html=preview></div>
                    </div>

                    <div class="panel editor-panel">
                        <h3>"Type Typst"</h3>
                        <div class="editor-wrap">
                            <pre class="hl" node_ref=hl_ref aria-hidden="true" inner_html=highlighted></pre>
                            <textarea
                                class="editor"
                                node_ref=ta_ref
                                spellcheck="false"
                                autocomplete="off"
                                on:input=on_input
                                on:keydown=on_keydown
                                on:scroll=on_scroll
                                placeholder="Type Typst here…"
                            ></textarea>
                        </div>
                        <div class="actions">
                            <button class="ghost" on:click=on_skip>"Skip"</button>
                            {(!timed).then(|| view! {
                                <button class="ghost" on:click=move |_| show_answer.update(|s| *s = !*s)>
                                    {move || if show_answer.get() { "Hide answer" } else { "Show answer" }}
                                </button>
                            })}
                            <span class="hint">"Correct answers are accepted automatically."</span>
                        </div>
                        {move || (!timed && show_answer.get()).then(|| view! {
                            <div class="answer">
                                <h3>"Answer"</h3>
                                <pre class="answer-src">{move || problem().map(|p| p.source)}</pre>
                            </div>
                        })}
                    </div>
                </div>
            }.into_any()
        }}

        {move || game_over.get().then(|| view! {
            <div class="modal">
                <div class="modal-box">
                    <button class="modal-close" title="Close" on:click=move |_| game_over.set(false)>"×"</button>
                    <h2>"Time's up!"</h2>
                    <p>
                        "You scored " <b>{move || score.get()}</b>
                        " points (" {move || solved.get()} " solved)."
                    </p>
                    {move || if submitted.get() {
                        view! {
                            <p class="submitted">"✓ Score submitted"</p>
                            <p><A href="/leaderboard">"View leaderboard →"</A></p>
                        }.into_any()
                    } else {
                        view! {
                            <input
                                placeholder="Your name"
                                prop:value=move || name.get()
                                on:input=move |ev| name.set(event_target_value(&ev))
                            />
                            <button
                                prop:disabled=move || finish_action.pending().get()
                                on:click=on_submit_score
                            >"Submit score"</button>
                            {move || submit_err.get().map(|e| view! { <p class="diag">{e}</p> })}
                        }.into_any()
                    }}
                    <div class="modal-actions">
                        <button class="ghost" on:click=play_again>"Play again"</button>
                        <A href="/leaderboard">"Leaderboard"</A>
                    </div>
                </div>
            </div>
        })}
    }
}
