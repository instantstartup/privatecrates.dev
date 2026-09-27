-- Migration number: 0001 	 Probe history for status.privatecrates.dev.

-- One row per environment per minute: component states and probe results, compact. Pruned after 90 days.
CREATE TABLE minutes (
	env TEXT NOT NULL,
	minute INTEGER NOT NULL, -- minutes since the Unix epoch, UTC
	states TEXT NOT NULL, -- one letter per component (o, d, x), in src/model.ts order
	probes TEXT NOT NULL, -- JSON [[status, ms], ...] for healthz, /api/status, canary
	github TEXT, -- JSON [requests, failed] from the server's /api/status, when known
	PRIMARY KEY (env, minute)
) WITHOUT ROWID;

-- One row per environment per UTC day: minutes operational, degraded and down per component (a DaySummary).
CREATE TABLE days (
	env TEXT NOT NULL,
	day TEXT NOT NULL, -- YYYY-MM-DD
	summary TEXT NOT NULL,
	PRIMARY KEY (env, day)
) WITHOUT ROWID;

-- The latest evaluation per environment, with the upstream snapshots it used: what the page shows now.
CREATE TABLE latest (
	env TEXT PRIMARY KEY,
	checked_at TEXT NOT NULL,
	evaluation TEXT NOT NULL,
	upstreams TEXT NOT NULL
);
