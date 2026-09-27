-- Who accepted which version of the PrivateCrates terms for which GitHub organisation (docs/preview.md §2).
-- Append-only: the server never updates or deletes a row. The first acceptance of a version is kept; a later one
-- is a no-op (`on conflict do nothing`).
create table terms_acceptances (
    id bigserial primary key,
    org_id bigint not null,
    org_login text not null,
    user_id bigint not null,
    user_login text not null,
    version text not null,
    accepted_at timestamptz not null default now(),
    via text not null check (via in ('website', 'cli')),
    statement text not null,
    unique (org_id, version)
);
