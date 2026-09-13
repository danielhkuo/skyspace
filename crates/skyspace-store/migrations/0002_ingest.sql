-- Ingest bookkeeping: one row per job run, one per fetched document, and the
-- parse issues and fill-rate statistics the guards read.

create table ingest_runs (
    id           bigint generated always as identity primary key,
    job          text        not null,
    term_code    text,
    started_at   timestamptz not null,
    finished_at  timestamptz,
    outcome      text        check (outcome in ('ok', 'failed', 'quarantined')),
    requests     integer     not null default 0,
    targets      integer     not null default 0,
    failures     integer     not null default 0,
    bytes        bigint      not null default 0,
    rows_written bigint      not null default 0,
    error        text
);
create index ingest_runs_recent on ingest_runs (job, started_at desc);
-- Feeds data_version for the catalog ETag.
create index ingest_runs_term_ok on ingest_runs (term_code, finished_at desc) where outcome = 'ok';

-- One row per fetch. The bytes live in the file archive under sha256; this
-- table is the index over them. Append only: the app role holds no update or
-- delete grant (see the role block at the end of 0008_feedback.sql).
create table raw_responses (
    id           bigint generated always as identity primary key,
    run_id       bigint      not null references ingest_runs (id),
    source       text        not null check (source in (
                     'listing', 'catalog', 'section_xml', 'detail',
                     'reference', 'program_index', 'program')),
    url          text        not null,
    term_code    text,
    fetched_at   timestamptz not null,
    status       smallint    not null,
    content_type text,
    byte_len     integer     not null,
    sha256       bytea       not null,
    headers      jsonb       not null default '{}'
);
create index raw_responses_sha on raw_responses (sha256);
create index raw_responses_url_time on raw_responses (url, fetched_at desc);
create index raw_responses_run on raw_responses (run_id);

create table parse_issues (
    id       bigint generated always as identity primary key,
    run_id   bigint not null references ingest_runs (id) on delete cascade,
    url      text   not null,
    severity text   not null check (severity in ('warn', 'error')),
    code     text   not null,
    detail   jsonb  not null default '{}'
);
create index parse_issues_run on parse_issues (run_id, severity);

create table parse_stats (
    run_id      bigint  not null references ingest_runs (id) on delete cascade,
    source      text    not null,
    field       text    not null,
    rows_seen   integer not null,
    rows_filled integer not null,
    primary key (run_id, source, field)
);
