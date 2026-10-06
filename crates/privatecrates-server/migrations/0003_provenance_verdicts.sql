-- Provenance the compliance dashboard has verified, so it is downloaded and checked once per version rather than on
-- every report. The key is a SHA-256 fingerprint of the storage repository ID, crate, version, checksum, the
-- provenance file's digest and the owners file's blob: it names no crate, and changes if any of them does.
create table provenance_verdicts (
    fingerprint bytea primary key,
    checked_at timestamptz not null default now()
);
