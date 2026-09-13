-- Degree programs. Extraction writes a draft; only a person's approval writes
-- program_versions and requirements, so an unreviewed rule cannot reach a
-- student's plan.

create table program_drafts (
    id            bigint generated always as identity primary key,
    run_id        bigint      not null references ingest_runs (id),
    slug          text        not null,
    catalog_year  smallint    not null,
    source_url    text        not null,
    source_sha256 bytea       not null,
    body          jsonb       not null,
    state         text        not null default 'pending'
                              check (state in ('pending', 'approved', 'rejected')),
    reviewer      text,
    reviewed_at   timestamptz,
    created_at    timestamptz not null default now()
);
create index program_drafts_pending on program_drafts (catalog_year, slug) where state = 'pending';

-- Identity is stable across catalog years.
create table programs (
    id         uuid        primary key default gen_random_uuid(),
    slug       text        not null unique,
    kind       text        not null check (kind in (
                   'university', 'major', 'minor', 'certificate', 'concentration')),
    name       text        not null,
    credential text        not null default '',
    created_at timestamptz not null default now()
);

create table program_versions (
    program_id           uuid        not null references programs (id) on delete cascade,
    catalog_year         smallint    not null,
    source               text        not null default 'ga' check (source in ('ga', 'manual')),
    ga_url               text        not null,
    total_credits_cents  smallint,
    from_draft           bigint      references program_drafts (id),
    published_at         timestamptz not null,
    reviewed_by          text        not null,
    retired_requirements uuid[]      not null default '{}',
    primary key (program_id, catalog_year)
);

-- `kind` maps one to one onto `skyspace_core::program::RequirementBody`.
-- `distinct_departments`, `min_departments`, `description` and
-- `source_anchor` are additions to the documented columns, one per core
-- field that had no column.
create table requirements (
    id                uuid     primary key default gen_random_uuid(),
    program_id        uuid     not null,
    catalog_year      smallint not null,
    parent_id         uuid     references requirements (id) on delete cascade,
    ordinal           smallint not null,
    kind              text     not null check (kind in (
                          'all', 'select', 'course', 'credits', 'non_course',
                          'unverifiable', 'distinct_departments')),
    label             text     not null,
    hours_kind        text     check (hours_kind in ('fixed', 'range', 'either')),
    hours_min_cents   smallint,
    hours_max_cents   smallint,
    select_count      smallint,
    semesters         smallint not null default 1,
    min_credits_cents smallint,
    credit_scope      text     check (credit_scope in ('any', 'additional')),
    non_course_kind   text     check (non_course_kind in ('proficiency_exam', 'portfolio', 'other')),
    min_departments   smallint,
    description       text,
    filter            jsonb,
    source_text       text     not null,
    source_url        text     not null,
    source_anchor     text,
    fingerprint       text     not null,
    -- A published rule is never deleted: a re-publish that no longer
    -- carries its fingerprint marks it retired, so a plan's
    -- `requirement_id` and a `requirement_reports` row keep resolving.
    retired           boolean  not null default false,
    check ((kind = 'select') = (select_count is not null)),
    check ((kind = 'credits') = (min_credits_cents is not null)),
    check ((kind = 'credits') = (credit_scope is not null)),
    check ((kind in ('course', 'credits')) = (filter is not null)),
    check ((kind = 'course') or semesters = 1),
    check ((kind = 'non_course') = (non_course_kind is not null)),
    check ((kind = 'non_course') = (description is not null)),
    check ((kind = 'distinct_departments') = (min_departments is not null)),
    check ((hours_kind is null) = (hours_min_cents is null)),
    check ((hours_kind is null) = (hours_max_cents is null)),
    foreign key (program_id, catalog_year)
        references program_versions (program_id, catalog_year) on delete cascade,
    unique (program_id, catalog_year, fingerprint)
);
create index requirements_tree on requirements (program_id, catalog_year, parent_id, ordinal);

-- Derived from `filter` on every publish, Code selectors only. Never the
-- source of truth.
create table requirement_courses (
    requirement_id uuid not null references requirements (id) on delete cascade,
    subject        text not null,
    number         text not null,
    primary key (requirement_id, subject, number)
);
create index requirement_courses_code on requirement_courses (subject, number);
