-- Mutable current seat counts for freshness, immutable change history for
-- charts, and the detail page's reserved-seat groups.

create table section_seat_state (
    section_id      bigint      primary key references sections (id) on delete cascade,
    enrolled        integer     not null,
    capacity        integer     not null,
    wait_count      integer     not null,
    wait_capacity   integer     not null,
    source_time     timestamptz not null,
    last_polled_at  timestamptz not null,
    last_changed_at timestamptz not null
);

-- Append only, one row per observed change. The app role holds no update or
-- delete grant on this table.
create table seat_snapshots (
    section_id    bigint      not null references sections (id) on delete cascade,
    observed_at   timestamptz not null,
    enrolled      integer     not null,
    capacity      integer     not null,
    wait_count    integer     not null,
    wait_capacity integer     not null,
    primary key (section_id, observed_at)
);
create index seat_snapshots_recent on seat_snapshots (observed_at desc);

create table section_seat_reservations (
    section_id bigint   not null references sections (id) on delete cascade,
    ordinal    smallint not null,
    label      text     not null,
    capacity   integer  not null,
    available  integer  not null,
    primary key (section_id, ordinal)
);
