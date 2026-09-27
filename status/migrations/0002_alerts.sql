-- Migration number: 0002 	 Alerting state: what we last told the owner, so nothing is paged twice (src/alerting.ts).

-- The latest cron run that alerted, per environment. A run claims its minute here first; a run for the same or an
-- earlier minute (overlapping, retried or late) then finds it taken and says nothing.
CREATE TABLE alert_runs (
	env TEXT PRIMARY KEY,
	minute INTEGER NOT NULL -- minutes since the Unix epoch, UTC
);

-- Per environment and component: what the last run saw, and what we last told the owner (an AlertRow).
-- Written only when it changes, so a steady state costs one row a minute per environment (alert_runs).
CREATE TABLE alert_state (
	env TEXT NOT NULL,
	component TEXT NOT NULL,
	observed TEXT NOT NULL, -- 'ok', or kind:state: 'ours:outage', 'upstream:degraded', ...
	observed_since INTEGER NOT NULL, -- ms since the Unix epoch
	streak INTEGER NOT NULL, -- consecutive runs of the observed kind, capped
	alerted TEXT NOT NULL, -- as observed: what we last said
	alerted_since INTEGER, -- when that problem began (ms)
	alerted_cause TEXT,
	alerted_reasons TEXT NOT NULL DEFAULT '[]', -- JSON Reason[]
	paged_at INTEGER, -- the last message other than a recovery (ms): flap limit and reminders
	PRIMARY KEY (env, component)
) WITHOUT ROWID;

-- Test pages sent (pnpm alert:test sets the ALERT_TEST secret to a fresh value; each value is sent once).
CREATE TABLE alert_tests (
	id TEXT PRIMARY KEY,
	sent_at TEXT NOT NULL
);
