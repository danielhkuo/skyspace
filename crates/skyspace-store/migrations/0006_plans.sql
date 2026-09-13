-- The student's documents, plus two ingest tables that reference terms.

create table plans (
    id           uuid        primary key default gen_random_uuid(),
    account_id   uuid        not null references accounts (id) on delete cascade,
    name         text        not null,
    catalog_year smallint    not null,
    is_active    boolean     not null default false,
    version      integer     not null default 1,
    body         jsonb       not null,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now()
);
create index plans_account on plans (account_id);
create unique index plans_one_active_per_account on plans (account_id) where is_active;
create index plans_programs on plans using gin ((body -> 'programs'));

create table schedules (
    id         uuid        primary key default gen_random_uuid(),
    account_id uuid        not null references accounts (id) on delete cascade,
    client_id  uuid,
    term_code  text        not null references terms (code),
    name       text        not null,
    version    integer     not null default 1,
    body       jsonb       not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);
create index schedules_account on schedules (account_id, term_code);
create unique index schedules_client on schedules (account_id, client_id) where client_id is not null;

-- The one denormalised table: it is the seat poll-set query.
create table schedule_sections (
    schedule_id uuid   not null references schedules (id) on delete cascade,
    section_id  bigint not null references sections (id) on delete cascade,
    primary key (schedule_id, section_id)
);
create index schedule_sections_section on schedule_sections (section_id);

create table collections (
    id         uuid        primary key default gen_random_uuid(),
    account_id uuid        not null references accounts (id) on delete cascade,
    client_id  uuid,
    name       text        not null,
    created_at timestamptz not null default now()
);
create unique index collections_name on collections (account_id, lower(name));
create unique index collections_client on collections (account_id, client_id) where client_id is not null;

-- Course code, not course_id: a bookmark survives a term where the course is
-- not offered.
create table collection_courses (
    collection_id uuid        not null references collections (id) on delete cascade,
    subject       text        not null,
    number        text        not null,
    added_at      timestamptz not null default now(),
    primary key (collection_id, subject, number)
);
create index collection_courses_code on collection_courses (subject, number);

create table poll_windows (
    id               bigint generated always as identity primary key,
    term_code        text        not null references terms (code),
    label            text        not null,
    starts_at        timestamptz not null,
    ends_at          timestamptz not null,
    interval_minutes smallint    not null default 15
);
create index poll_windows_active on poll_windows (starts_at, ends_at);

create table canaries (
    id           bigint generated always as identity primary key,
    source       text    not null,
    term_code    text    references terms (code),
    url          text    not null,
    expect_rows  integer,
    expect_field text,
    note         text    not null default ''
);
