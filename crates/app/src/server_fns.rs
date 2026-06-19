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

/// Time window for leaderboard queries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeaderboardPeriod {
    #[default]
    AllTime,
    Monthly,
    Daily,
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
    /// Why the solve was rejected (`None` when accepted) — shown to the player.
    pub reason: Option<String>,
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
    /// Sessions older than this are eligible for reaping. Comfortably exceeds
    /// `GAME_SECONDS + GRACE_SECONDS` so no live game is ever reaped.
    pub const SESSION_TTL_SECONDS: u64 = 600;
    /// How much longer the client may *claim* a solve took than the server saw.
    pub const ELAPSED_SLACK_MS: u32 = 750;

    /// Server-side state for one in-progress game.
    pub struct Session {
        pub problems: Vec<Problem>,
        pub score: u32,
        pub solved: u32,
        pub start: Instant,
        pub done: HashSet<usize>,
        /// The problem index the server last handed out — the only one a solve
        /// may be credited for (set by `get_problem`).
        pub current: Option<usize>,
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

    /// Remove sessions that have been running longer than [`SESSION_TTL_SECONDS`].
    /// Returns the number of sessions removed. Called periodically by the server's
    /// background reaper to bound memory growth from abandoned games.
    ///
    /// # Panics
    /// Panics if the sessions mutex is poisoned (i.e. a previous holder panicked
    /// while holding the lock, which should never happen in practice).
    pub fn reap_stale(store: &Sessions) -> usize {
        let mut map = store.lock().expect("sessions lock");
        let before = map.len();
        map.retain(|_, s| s.start.elapsed().as_secs() <= SESSION_TTL_SECONDS);
        before - map.len()
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
        // Only the problem the server last served can be credited — the client
        // can't bank solves for problems it isn't currently on.
        if s.current != Some(index) {
            return SolveOutcome::Rejected("not the active problem");
        }
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
            current: None,
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
    s.current = Some(idx);
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
    let mut reason = None;
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
                },
                Err(e) => tracing::error!(request_id, error = %e, "no pool to record solve"),
            }
        },
        SolveOutcome::Rejected(why) => {
            tracing::warn!(
                request_id,
                session,
                index,
                reason = why,
                client_ms = meta.elapsed_ms,
                server_ms = ?server_elapsed_ms,
                typed_chars = meta.typed_chars,
                keydowns = meta.keydowns,
                stddev_interval_ms = meta.stddev_interval_ms,
                "solve rejected"
            );
            reason = Some(why.to_string());
        },
    }

    Ok(SolveResult {
        accepted,
        score,
        solved,
        reason,
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

/// Top scores, highest first. `period` filters to scores submitted within the
/// last day, last 30 days, or all time.
#[server]
pub async fn top_scores(period: LeaderboardPeriod) -> Result<Vec<ScoreEntry>, ServerFnError> {
    use ssr::{new_request_id, pool, req_err};

    let request_id = new_request_id();
    let pool = pool()?;

    // WHERE clause comes from a trusted server-side enum, not user input.
    let since = match period {
        LeaderboardPeriod::AllTime => String::new(),
        LeaderboardPeriod::Monthly => "WHERE created_at >= datetime('now', '-30 days') ".into(),
        LeaderboardPeriod::Daily => "WHERE created_at >= datetime('now', '-1 day') ".into(),
    };
    let sql = format!(
        "SELECT name, score, problems_solved FROM scores \
         {since}ORDER BY score DESC, created_at ASC LIMIT 50"
    );

    let rows = sqlx::query_as::<_, (String, i64, i64)>(&sql)
        .fetch_all(&pool)
        .await
        .map_err(|e| req_err(&request_id, &format!("database error: {e}")))?;

    tracing::debug!(request_id, count = rows.len(), ?period, "leaderboard read");
    Ok(rows
        .into_iter()
        .map(|(name, score, problems_solved)| ScoreEntry {
            name,
            score,
            problems_solved,
        })
        .collect())
}

/// Save a custom practice set (a list of problem titles) server-side and return
/// its UUID. The UUID forms a permanent `/practice/{id}` link: the set is
/// looked up by ID even if problems are added or removed later. Titles that no
/// longer exist simply won't appear when the set is loaded.
#[server]
pub async fn save_practice_set(titles: Vec<String>) -> Result<String, ServerFnError> {
    use ssr::{new_request_id, pool, req_err};

    let request_id = new_request_id();

    if titles.is_empty() {
        return Err(req_err(&request_id, "practice set must not be empty"));
    }
    if titles.len() > 300 {
        return Err(req_err(&request_id, "practice set too large"));
    }

    // Validate every title against the bundled problem set so clients can't
    // store arbitrary strings.
    let known: std::collections::HashSet<String> = crate::problems::load_problems()
        .into_iter()
        .map(|p| p.title)
        .collect();
    for t in &titles {
        if !known.contains(t) {
            return Err(req_err(&request_id, &format!("unknown problem: {t}")));
        }
    }

    let pool = pool()?;
    let id = uuid::Uuid::new_v4().to_string();
    let json =
        serde_json::to_string(&titles).map_err(|e| req_err(&request_id, &format!("json: {e}")))?;
    sqlx::query("INSERT INTO practice_sets (id, titles) VALUES (?, ?)")
        .bind(&id)
        .bind(&json)
        .execute(&pool)
        .await
        .map_err(|e| req_err(&request_id, &format!("db: {e}")))?;

    tracing::debug!(request_id, count = titles.len(), %id, "practice set saved");
    Ok(id)
}

/// Load a previously saved practice set by UUID. Returns the list of problem
/// titles; the caller filters `load_problems()` by this list.
#[server]
pub async fn get_practice_set(id: String) -> Result<Vec<String>, ServerFnError> {
    use ssr::{new_request_id, pool, req_err};

    let request_id = new_request_id();
    let pool = pool()?;
    let row: Option<(String,)> = sqlx::query_as("SELECT titles FROM practice_sets WHERE id = ?")
        .bind(&id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| req_err(&request_id, &format!("db: {e}")))?;

    match row {
        None => Err(req_err(&request_id, "practice set not found")),
        Some((json,)) => {
            serde_json::from_str(&json).map_err(|e| req_err(&request_id, &format!("json: {e}")))
        },
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::ssr::{
        credit_solve, new_sessions, plausible, reap_stale, sanitize_name, Session, SolveOutcome,
    };
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
            // The active problem is index 0 (as if `get_problem(0)` was served).
            current: Some(0),
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
    fn rejects_solve_for_non_active_problem() {
        let mut s = session_with("a + b");
        s.problems.push(crate::problems::Problem {
            title: "u".into(),
            kind: typst_engine::Kind::Math,
            source: "c + d".into(),
        });
        // Active problem is index 0; a correct solve for index 1 is rejected.
        assert!(matches!(
            credit_solve(&mut s, 1, "c + d", &good_meta(5), Some(4200)),
            SolveOutcome::Rejected("not the active problem")
        ));
        assert_eq!(s.solved, 0);
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

    #[test]
    fn reap_stale_removes_expired_sessions() {
        let store = new_sessions();
        // A session whose `start` is well in the past (far beyond SESSION_TTL_SECONDS).
        let stale_start = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(700))
            .expect("instant subtraction");
        {
            let mut map = store.lock().unwrap();
            map.insert(
                "stale".into(),
                Session {
                    problems: vec![],
                    score: 0,
                    solved: 0,
                    start: stale_start,
                    done: std::collections::HashSet::new(),
                    current: None,
                    fetched_at: std::collections::HashMap::new(),
                },
            );
            map.insert("fresh".into(), session_with("x"));
        }
        let removed = reap_stale(&store);
        assert_eq!(removed, 1);
        let map = store.lock().unwrap();
        assert!(map.contains_key("fresh"));
        assert!(!map.contains_key("stale"));
    }
}
