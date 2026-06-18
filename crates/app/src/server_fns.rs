//! Server-authoritative game sessions, scoring, telemetry, and the leaderboard.
//!
//! The browser still renders everything (it receives the problems, answers
//! included), but the **score lives on the server**. When the client thinks it
//! solved a problem it calls [`solve`] with the typed answer plus keystroke
//! telemetry; the server re-compiles and matches the answer, runs
//! human-plausibility checks (including cross-checking the client's claimed
//! time against the real server wall-clock), and only then credits points and
//! records the solve. The leaderboard is written from the server's own tally
//! via [`finish_game`] — the client never supplies a score number.
//!
//! Every server function mints a request UUID, logs structured `tracing` events
//! tagged with it (and the session), and embeds it in any returned error so the
//! browser can log it for end-to-end correlation.
//!
//! Server-only state (sessions, DB pool, tracing) lives behind `cfg(ssr)`; the
//! `#[server]` macro keeps those bodies off the client.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::problems::Problem;

/// One leaderboard row.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScoreEntry {
    pub name: String,
    pub score: i64,
    pub problems_solved: i64,
}

/// Result of starting a game: a session token and how many problems it holds.
/// Problems themselves are fetched one at a time with [`get_problem`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameStart {
    pub session: String,
    pub total: usize,
}

/// Client-reported telemetry about how an answer was entered. All fields are
/// client-supplied (hence forgeable), but together they make trivial automation
/// and paste-the-answer much harder, and they're stored for later analysis.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct InputMeta {
    /// Characters in the final answer the player typed (catches paste).
    pub typed_chars: u32,
    /// Milliseconds from the problem appearing to the answer being correct.
    pub elapsed_ms: u32,
    /// Total keydown events (including edits/backspaces).
    pub keydowns: u32,
    /// Number of Backspace/Delete presses.
    pub backspaces: u32,
    /// Delay from the problem appearing to the first keystroke.
    pub first_key_ms: u32,
    /// Mean gap between consecutive keystrokes.
    pub mean_interval_ms: u32,
    /// Standard deviation of the inter-keystroke gaps (low ⇒ robotic).
    pub stddev_interval_ms: u32,
    /// Smallest gap between two consecutive keystrokes.
    pub min_interval_ms: u32,
}

/// Server response to a solve attempt. `score`/`solved` are the authoritative
/// totals; `request_id` correlates this attempt with the server's trace log, so
/// the client can log it (e.g. on a rejection) and we can look up the reason.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SolveResult {
    pub accepted: bool,
    pub score: u32,
    pub solved: u32,
    pub request_id: String,
}

#[cfg(feature = "ssr")]
pub mod ssr {
    use std::collections::{HashMap, HashSet};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use leptos::prelude::*;

    pub use sqlx::SqlitePool;

    use crate::problems::Problem;

    /// A 3-minute game; allow a little slack for network/clock skew.
    pub const GAME_SECONDS: u64 = 180;
    pub const GRACE_SECONDS: u64 = 10;
    /// How much longer the client may *claim* a solve took than the server saw.
    pub const ELAPSED_SLACK_MS: u32 = 750;

    /// Server-side state for one in-progress game.
    pub struct Session {
        pub problems: Vec<Problem>,
        pub score: u32,
        pub solved: u32,
        pub start: Instant,
        pub done: HashSet<usize>,
        /// When the client last fetched each problem (for the timing cross-check).
        pub fetched_at: HashMap<usize, Instant>,
    }

    /// Data recorded for an accepted solve.
    pub struct SolveRecord {
        pub problem_title: String,
        pub problem_index: usize,
        pub points: u32,
        pub server_elapsed_ms: Option<u32>,
    }

    /// The decision made by [`credit_solve`], carrying either the recorded solve
    /// or the (loggable) reason it was rejected.
    pub enum SolveOutcome {
        Accepted(SolveRecord),
        Rejected(&'static str),
    }

    /// In-memory session store. Ephemeral: cleared on restart, single-instance only.
    pub type Sessions = Arc<Mutex<HashMap<String, Session>>>;

    #[must_use]
    pub fn new_sessions() -> Sessions {
        Arc::new(Mutex::new(HashMap::new()))
    }

    /// A random session token (UUID v4).
    #[must_use]
    pub fn new_session_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    /// A random per-request id (UUID v4), for log correlation across client/server.
    #[must_use]
    pub fn new_request_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    /// Build a `ServerFnError` carrying the request id (so the browser can log
    /// it) and log a matching `warn` on the server.
    pub fn req_err(request_id: &str, msg: &str) -> ServerFnError {
        tracing::warn!(request_id, "{msg}");
        ServerFnError::new(format!("[req {request_id}] {msg}"))
    }

    /// # Errors
    /// Errors if the pool was not provided to the request context.
    pub fn pool() -> Result<SqlitePool, ServerFnError> {
        use_context::<SqlitePool>()
            .ok_or_else(|| ServerFnError::new("database pool not found in context"))
    }

    /// # Errors
    /// Errors if the session store was not provided to the request context.
    pub fn sessions() -> Result<Sessions, ServerFnError> {
        use_context::<Sessions>()
            .ok_or_else(|| ServerFnError::new("session store not found in context"))
    }

    /// Core solve logic (no request context): validate a solve attempt against a
    /// session and credit points if genuine. On success also returns a
    /// [`SolveRecord`] to persist. Separated out so it is unit-testable.
    #[must_use]
    pub fn credit_solve(
        s: &mut Session,
        index: usize,
        answer: &str,
        meta: &super::InputMeta,
        server_elapsed_ms: Option<u32>,
    ) -> SolveOutcome {
        if s.start.elapsed().as_secs() > GAME_SECONDS + GRACE_SECONDS {
            return SolveOutcome::Rejected("game already over");
        }
        let Some(problem) = s.problems.get(index).cloned() else {
            return SolveOutcome::Rejected("invalid problem index");
        };
        if s.done.contains(&index) {
            return SolveOutcome::Rejected("already solved");
        }
        if let Some(reason) = plausible(meta, answer.len(), server_elapsed_ms) {
            return SolveOutcome::Rejected(reason);
        }
        if !typst_engine::matches(&problem.source, answer, problem.kind) {
            return SolveOutcome::Rejected("answer does not match");
        }

        s.done.insert(index);
        let points = typst_engine::difficulty_score(&problem.source);
        s.score += points;
        s.solved += 1;
        SolveOutcome::Accepted(SolveRecord {
            problem_title: problem.title,
            problem_index: index,
            points,
            server_elapsed_ms,
        })
    }

    /// Heuristic "did a human type this?" check on the reported input metadata,
    /// cross-checked against the server-measured time since the problem was sent.
    ///
    /// All inputs are client-reportable, so this isn't bulletproof — it raises
    /// the bar against trivial automation and paste-the-answer.
    /// Returns `None` if the attempt looks human, or `Some(reason)` describing
    /// why it was rejected (logged server-side for tracing).
    #[must_use]
    pub fn plausible(
        meta: &super::InputMeta,
        answer_len: usize,
        server_elapsed_ms: Option<u32>,
    ) -> Option<&'static str> {
        // Took at least a moment — not an instant script.
        if meta.elapsed_ms < 400 {
            return Some("too fast (<400ms)");
        }
        // Actually typed (roughly) the whole answer; a paste reports ~no keystrokes.
        if (meta.typed_chars as usize) + 2 < answer_len {
            return Some("too few keystrokes (paste?)");
        }
        // Below a superhuman typing speed.
        let seconds = f64::from(meta.elapsed_ms) / 1000.0;
        if f64::from(meta.typed_chars) / seconds > 25.0 {
            return Some("superhuman typing speed");
        }
        // Timing cross-check: the client can't truthfully claim it spent *more*
        // time than the server observed since it sent the problem.
        if let Some(server_ms) = server_elapsed_ms {
            if meta.elapsed_ms > server_ms + ELAPSED_SLACK_MS {
                return Some("claimed time exceeds server window");
            }
        }
        // Robotic, near-constant keystroke rhythm over many keys.
        if meta.keydowns > 8 && meta.stddev_interval_ms < 5 {
            return Some("near-constant keystroke rhythm (bot)");
        }
        None
    }

    /// Persist an accepted solve with its telemetry.
    ///
    /// # Errors
    /// Propagates any database error.
    pub async fn insert_solve(
        pool: &SqlitePool,
        session: &str,
        rec: &SolveRecord,
        meta: &super::InputMeta,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO solves (session, problem_title, problem_index, points, \
             server_elapsed_ms, client_elapsed_ms, typed_chars, keydowns, backspaces, \
             first_key_ms, mean_interval_ms, stddev_interval_ms, min_interval_ms) \
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(session)
        .bind(&rec.problem_title)
        .bind(i64::try_from(rec.problem_index).unwrap_or(-1))
        .bind(i64::from(rec.points))
        .bind(rec.server_elapsed_ms.map(i64::from))
        .bind(i64::from(meta.elapsed_ms))
        .bind(i64::from(meta.typed_chars))
        .bind(i64::from(meta.keydowns))
        .bind(i64::from(meta.backspaces))
        .bind(i64::from(meta.first_key_ms))
        .bind(i64::from(meta.mean_interval_ms))
        .bind(i64::from(meta.stddev_interval_ms))
        .bind(i64::from(meta.min_interval_ms))
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Strip control chars, collapse whitespace, cap length; default if empty.
    #[must_use]
    pub fn sanitize_name(name: &str) -> String {
        const MAX_NAME_LEN: usize = 24;
        let cleaned = name.chars().filter(|c| !c.is_control()).collect::<String>();
        let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
        if cleaned.is_empty() {
            return "anonymous".to_string();
        }
        cleaned.chars().take(MAX_NAME_LEN).collect()
    }
}

/// Begin a game: create a server session and return the number of problems.
#[server]
#[allow(clippy::unused_async)] // `#[server]` functions must be async
pub async fn start_game() -> Result<GameStart, ServerFnError> {
    use rand::seq::SliceRandom;
    use ssr::{new_request_id, new_session_id, sessions, Session};

    let request_id = new_request_id();
    let store = sessions()?;
    let mut problems = crate::problems::load_problems();
    problems.shuffle(&mut rand::thread_rng());

    let id = new_session_id();
    let total = problems.len();
    store.lock().expect("session lock").insert(
        id.clone(),
        Session {
            problems,
            score: 0,
            solved: 0,
            start: std::time::Instant::now(),
            done: std::collections::HashSet::new(),
            fetched_at: std::collections::HashMap::new(),
        },
    );
    tracing::info!(request_id, session = id, total, "game started");
    Ok(GameStart { session: id, total })
}

/// Fetch one problem (with its answer, for client rendering) by its index in the
/// server's order, recording when it was sent for the later timing cross-check.
#[server]
#[allow(clippy::unused_async)] // `#[server]` functions must be async
pub async fn get_problem(session: String, index: usize) -> Result<Problem, ServerFnError> {
    use ssr::{new_request_id, req_err, sessions};

    let request_id = new_request_id();
    let store = sessions()?;
    let mut guard = store.lock().expect("session lock");
    let s = guard
        .get_mut(&session)
        .ok_or_else(|| req_err(&request_id, "unknown session"))?;
    let n = s.problems.len();
    if n == 0 {
        return Err(req_err(&request_id, "session has no problems"));
    }
    let idx = index % n;
    s.fetched_at.insert(idx, std::time::Instant::now());
    let problem = s.problems[idx].clone();
    tracing::debug!(request_id, session, index = idx, title = %problem.title, "fetch problem");
    Ok(problem)
}

/// Validate a solve attempt, credit points server-side if genuine, and record it.
#[server]
pub async fn solve(
    session: String,
    index: usize,
    answer: String,
    meta: InputMeta,
) -> Result<SolveResult, ServerFnError> {
    use ssr::{credit_solve, insert_solve, new_request_id, pool, req_err, sessions, SolveOutcome};

    let request_id = new_request_id();
    tracing::debug!(
        request_id,
        session,
        index,
        typed_chars = meta.typed_chars,
        client_ms = meta.elapsed_ms,
        "solve received"
    );

    // Synchronous credit while holding the session lock.
    let (outcome, score, solved, server_elapsed_ms) = {
        let store = sessions()?;
        let mut guard = store.lock().expect("session lock");
        let s = guard
            .get_mut(&session)
            .ok_or_else(|| req_err(&request_id, "unknown session"))?;
        let server_elapsed_ms = s
            .fetched_at
            .get(&(index % s.problems.len().max(1)))
            .map(|t| u32::try_from(t.elapsed().as_millis()).unwrap_or(u32::MAX));
        let outcome = credit_solve(s, index, &answer, &meta, server_elapsed_ms);
        (outcome, s.score, s.solved, server_elapsed_ms)
    };

    let accepted = matches!(outcome, SolveOutcome::Accepted(_));
    match outcome {
        SolveOutcome::Accepted(rec) => {
            tracing::info!(
                request_id,
                session,
                index,
                title = %rec.problem_title,
                points = rec.points,
                score,
                solved,
                client_ms = meta.elapsed_ms,
                server_ms = ?server_elapsed_ms,
                "solve accepted"
            );
            // Record the solve for later analysis (best-effort; never fail the call).
            match pool() {
                Ok(pool) => {
                    if let Err(e) = insert_solve(&pool, &session, &rec, &meta).await {
                        tracing::error!(request_id, session, error = %e, "failed to record solve");
                    }
                }
                Err(e) => tracing::error!(request_id, error = %e, "no pool to record solve"),
            }
        }
        SolveOutcome::Rejected(reason) => {
            tracing::warn!(
                request_id,
                session,
                index,
                reason,
                client_ms = meta.elapsed_ms,
                server_ms = ?server_elapsed_ms,
                typed_chars = meta.typed_chars,
                keydowns = meta.keydowns,
                stddev_interval_ms = meta.stddev_interval_ms,
                "solve rejected"
            );
        }
    }

    Ok(SolveResult {
        accepted,
        score,
        solved,
        request_id,
    })
}

/// Finish a game: write the session's server-tracked score to the leaderboard.
#[server]
pub async fn finish_game(session: String, name: String) -> Result<(), ServerFnError> {
    use ssr::{new_request_id, pool, req_err, sanitize_name, sessions};

    let request_id = new_request_id();
    let (score, solved) = {
        let store = sessions()?;
        let mut guard = store.lock().expect("session lock");
        let s = guard
            .remove(&session)
            .ok_or_else(|| req_err(&request_id, "unknown session"))?;
        (i64::from(s.score), i64::from(s.solved))
    };

    let name = sanitize_name(&name);
    tracing::info!(request_id, session, score, solved, name, "game finished");

    let pool = pool()?;
    sqlx::query("INSERT INTO scores (name, score, problems_solved) VALUES (?, ?, ?)")
        .bind(name)
        .bind(score)
        .bind(solved)
        .execute(&pool)
        .await
        .map_err(|e| req_err(&request_id, &format!("database error: {e}")))?;
    Ok(())
}

/// Top scores, highest first.
#[server]
pub async fn top_scores() -> Result<Vec<ScoreEntry>, ServerFnError> {
    use ssr::{new_request_id, pool, req_err};

    let request_id = new_request_id();
    let pool = pool()?;

    let rows = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT name, score, problems_solved FROM scores ORDER BY score DESC LIMIT 20",
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| req_err(&request_id, &format!("database error: {e}")))?;

    tracing::debug!(request_id, count = rows.len(), "leaderboard read");
    Ok(rows
        .into_iter()
        .map(|(name, score, problems_solved)| ScoreEntry {
            name,
            score,
            problems_solved,
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::ssr::{credit_solve, plausible, sanitize_name, Session, SolveOutcome};
    use super::InputMeta;

    fn good_meta(len: usize) -> InputMeta {
        InputMeta {
            typed_chars: len as u32,
            elapsed_ms: 4000,
            keydowns: len as u32,
            backspaces: 0,
            first_key_ms: 300,
            mean_interval_ms: 200,
            stddev_interval_ms: 80,
            min_interval_ms: 30,
        }
    }

    fn session_with(source: &str) -> Session {
        Session {
            problems: vec![crate::problems::Problem {
                title: "t".into(),
                kind: typst_engine::Kind::Math,
                source: source.into(),
            }],
            score: 0,
            solved: 0,
            start: std::time::Instant::now(),
            done: std::collections::HashSet::new(),
            fetched_at: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn plausible_accepts_human_typing() {
        assert!(plausible(&good_meta(16), 16, Some(5000)).is_none());
    }

    #[test]
    fn plausible_rejects_with_reasons() {
        // Instant.
        assert!(plausible(
            &InputMeta {
                elapsed_ms: 50,
                ..good_meta(16)
            },
            16,
            None
        )
        .is_some());
        // Pasted: barely any keystrokes for a long answer.
        assert!(plausible(
            &InputMeta {
                typed_chars: 1,
                ..good_meta(1)
            },
            30,
            None
        )
        .is_some());
        // Superhuman speed.
        assert!(plausible(
            &InputMeta {
                typed_chars: 500,
                elapsed_ms: 1000,
                ..good_meta(500)
            },
            30,
            None
        )
        .is_some());
        // Claims more time than the server observed (forged elapsed).
        assert_eq!(
            plausible(&good_meta(16), 16, Some(300)),
            Some("claimed time exceeds server window")
        );
        // Robotic, zero-variance rhythm.
        assert_eq!(
            plausible(
                &InputMeta {
                    keydowns: 20,
                    stddev_interval_ms: 0,
                    ..good_meta(16)
                },
                16,
                Some(5000)
            ),
            Some("near-constant keystroke rhythm (bot)")
        );
    }

    #[test]
    fn credits_a_genuine_solve_once() {
        let mut s = session_with("a + b");
        let outcome = credit_solve(&mut s, 0, "a + b", &good_meta(5), Some(4200));
        assert!(matches!(outcome, SolveOutcome::Accepted(_)) && s.solved == 1 && s.score > 0);
        // Re-solving the same problem is rejected with a reason.
        let again = credit_solve(&mut s, 0, "a + b", &good_meta(5), Some(4200));
        assert!(matches!(again, SolveOutcome::Rejected("already solved")));
    }

    #[test]
    fn rejects_wrong_answer_and_paste() {
        let mut s = session_with("a + b");
        assert!(matches!(
            credit_solve(&mut s, 0, "a - b", &good_meta(5), Some(4200)),
            SolveOutcome::Rejected("answer does not match")
        ));
        let pasted = InputMeta {
            typed_chars: 1,
            ..good_meta(1)
        };
        assert!(matches!(
            credit_solve(&mut s, 0, "a + b", &pasted, Some(4200)),
            SolveOutcome::Rejected(_)
        ));
        assert_eq!(s.score, 0);
    }

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_name("  spaced   out \n"), "spaced out");
        assert_eq!(sanitize_name(""), "anonymous");
        assert_eq!(sanitize_name("a\u{0}b\u{7}c"), "abc");
        assert_eq!(sanitize_name(&"x".repeat(100)).chars().count(), 24);
    }
}
