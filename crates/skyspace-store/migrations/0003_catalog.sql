-- The catalog spine: terms, courses, per-year catalog records, sections and
-- their meetings and instructors.

create extension if not exists pg_trgm;

create table terms (
    code          text     primary key,
    academic_year smallint not null,
    season        text     check (season in ('fall', 'spring', 'summer')),
    label         text     not null,
    is_current    boolean  not null default false,
    first_seen_at timestamptz not null default now()
);
-- At most one current term, enforced in the database.
create unique index terms_one_current on terms ((is_current)) where is_current;

create table courses (
    id         bigint generated always as identity primary key,
    subject    text        not null,
    number     text        not null,
    title      text        not null,
    code_norm  text        generated always as (lower(subject || number)) stored,
    first_seen timestamptz not null default now(),
    last_seen  timestamptz not null default now(),
    unique (subject, number)
);
create index courses_code_norm_trgm on courses using gin (code_norm gin_trgm_ops);
create index courses_title_fts on courses using gin (to_tsvector('english', title));

-- One CATALIST record per course per academic year. Prerequisites live here
-- because Rice publishes them per catalog year. `restrictions`, `equivalents`
-- and `second_half` are additions to the documented columns so a
-- `skyspace_core::catalog::Course` round-trips without loss.
create table course_catalog (
    course_id             bigint      not null references courses (id),
    catalog_year          smallint    not null,
    department            text        not null,
    credits_kind          text        not null check (credits_kind in ('fixed', 'range', 'either')),
    credits_min_cents     smallint    not null,
    credits_max_cents     smallint    not null,
    attributes            text[]      not null default '{}',
    grade_mode            text,
    course_type           text,
    language              text,
    restriction_text      text,
    restrictions          jsonb,
    prerequisite_text     text,
    prerequisite_expr     jsonb,
    corequisite_subject   text,
    corequisite_number    text,
    description           text        not null,
    repeatable            boolean     not null default false,
    instructor_permission boolean     not null default false,
    second_half           boolean     not null default false,
    equivalents           text[]      not null default '{}',
    fetched_at            timestamptz not null,
    primary key (course_id, catalog_year)
);
create index course_catalog_year on course_catalog (catalog_year);
create index course_catalog_attributes on course_catalog using gin (attributes);

-- One course under two codes. `CourseFacts::canonical` is built from this.
create table course_aliases (
    alias_subject     text not null,
    alias_number      text not null,
    canonical_subject text not null,
    canonical_number  text not null,
    primary key (alias_subject, alias_number)
);

-- Flat index of every code in a prerequisite expression; the tree stays in
-- course_catalog.prerequisite_expr.
create table course_prerequisites (
    course_id    bigint   not null references courses (id),
    catalog_year smallint not null,
    ordinal      smallint not null,
    subject      text     not null,
    number       text     not null,
    primary key (course_id, catalog_year, ordinal)
);
create index course_prerequisites_code on course_prerequisites (subject, number);

create table course_exclusions (
    course_id    bigint   not null references courses (id),
    catalog_year smallint not null,
    ordinal      smallint not null,
    subject      text     not null,
    number       text     not null,
    published    text     not null,
    primary key (course_id, catalog_year, ordinal)
);

-- `school` and `detail` are additions: the school filter has no other home,
-- and `detail` keeps the whole `SectionDetail` document so the class page
-- can show it without a lossy rebuild.
create table sections (
    id                 bigint generated always as identity primary key,
    term_code          text        not null references terms (code),
    crn                integer     not null,
    course_id          bigint      not null references courses (id),
    section_code       text        not null,
    title              text        not null,
    part_of_term_label text,
    part_of_term       text,
    credits_kind       text        not null check (credits_kind in ('fixed', 'range', 'either')),
    credits_min_cents  smallint    not null,
    credits_max_cents  smallint    not null,
    attributes         text[]      not null default '{}',
    school             text,
    final_exam         text,
    xml_fetched_at     timestamptz,
    detail_fetched_at  timestamptz,
    detail             jsonb,
    fees_text          text,
    has_syllabus       boolean,
    first_seen_at      timestamptz not null default now(),
    last_seen_at       timestamptz not null default now(),
    withdrawn_at       timestamptz,
    unique (term_code, crn)
);
create index sections_term_course on sections (term_code, course_id);
create index sections_attributes on sections using gin (attributes);
create index sections_xml_due on sections (xml_fetched_at nulls first) where withdrawn_at is null;
create index sections_detail_due on sections (detail_fetched_at nulls first) where withdrawn_at is null;

-- day_mask bit 0 is Monday. `raw` is kept so an unparsed meeting is still shown.
create table meetings (
    id         bigint generated always as identity primary key,
    section_id bigint   not null references sections (id) on delete cascade,
    kind       text     not null check (kind in ('class', 'final')),
    day_mask   smallint not null default 0,
    start_time time,
    end_time   time,
    start_date date,
    end_date   date,
    raw        text     not null
);
create index meetings_section on meetings (section_id);
create index meetings_window on meetings (start_time, end_time) where kind = 'class';

create table instructors (
    id    bigint generated always as identity primary key,
    name  text not null unique,
    netid text
);
create index instructors_name_trgm on instructors using gin (name gin_trgm_ops);

create table section_instructors (
    section_id    bigint   not null references sections (id) on delete cascade,
    instructor_id bigint   not null references instructors (id),
    ordinal       smallint not null,
    primary key (section_id, instructor_id)
);
create index section_instructors_instructor on section_instructors (instructor_id);
