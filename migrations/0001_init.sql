CREATE TABLE IF NOT EXISTS scores (
    id              INTEGER PRIMARY KEY,
    name            TEXT    NOT NULL,
    score           INTEGER NOT NULL,
    problems_solved INTEGER NOT NULL,
    created_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_scores_score ON scores (score DESC);
