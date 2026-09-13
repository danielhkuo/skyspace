-- Feedback on a rule, and product counters. No account id, session id or IP
-- address in either table.

create table requirement_reports (
    id             bigint generated always as identity primary key,
    requirement_id uuid references requirements (id) on delete set null,
    program_id     uuid references programs (id) on delete set null,
    catalog_year   smallint,
    message        text        not null,
    created_at     timestamptz not null default now(),
    resolved_at    timestamptz
);
create index requirement_reports_open on requirement_reports (created_at desc) where resolved_at is null;

create table usage_events (
    name      text        not null,
    term_code text,
    at        timestamptz not null default now(),
    detail    jsonb       not null default '{}'
);
create index usage_events_at on usage_events (name, at desc);

-- Roles. Migrations run as the owner; the application connects as
-- `skyspace_app`. The role itself is created out of band (it is
-- cluster-wide, not per database), so the grants are a commented block here
-- rather than statements a throwaway test database would have to satisfy:
--
--   create role skyspace_app login password '...';
--   grant usage on schema public to skyspace_app;
--   grant select, insert, update, delete on all tables in schema public to skyspace_app;
--   grant usage, select on all sequences in schema public to skyspace_app;
--   -- Append-only tables: take the write grants back.
--   revoke update, delete on seat_snapshots, raw_responses, parse_issues from skyspace_app;
