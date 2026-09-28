-- Requests to join the private preview (docs/preview.md §5): who asked, for which GitHub organisation, and how to
-- reply. The organisation is as typed: GitHub does not show us an organisation that has not installed our App.
-- One row per person and organisation; asking again updates it.
create table invitation_requests (
    id bigserial primary key,
    org_login text not null,
    user_id bigint not null,
    user_login text not null,
    email text not null,
    note text not null default '',
    requested_at timestamptz not null default now(),
    unique (org_login, user_id)
);
