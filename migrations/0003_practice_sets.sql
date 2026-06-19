CREATE TABLE IF NOT EXISTS practice_sets (
    id         TEXT PRIMARY KEY,
    titles     TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
