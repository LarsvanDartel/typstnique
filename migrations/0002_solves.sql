-- Per-solve telemetry for later analysis (accepted solves only).
CREATE TABLE IF NOT EXISTS solves (
    id                 INTEGER PRIMARY KEY,
    session            TEXT    NOT NULL,
    problem_title      TEXT    NOT NULL,
    problem_index      INTEGER NOT NULL,
    points             INTEGER NOT NULL,
    server_elapsed_ms  INTEGER,            -- NULL if the fetch time was unknown
    client_elapsed_ms  INTEGER NOT NULL,
    typed_chars        INTEGER NOT NULL,
    keydowns           INTEGER NOT NULL,
    backspaces         INTEGER NOT NULL,
    first_key_ms       INTEGER NOT NULL,
    mean_interval_ms   INTEGER NOT NULL,
    stddev_interval_ms INTEGER NOT NULL,
    min_interval_ms    INTEGER NOT NULL,
    created_at         TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_solves_session ON solves (session);
