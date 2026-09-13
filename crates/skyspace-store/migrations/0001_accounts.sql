-- Accounts. Email is a verified @rice.edu address; there is no password column
-- anywhere, because Skyspace never holds one.
create table accounts (
    id           uuid        primary key default gen_random_uuid(),
    email        text        not null unique,
    created_at   timestamptz not null default now(),
    last_seen_at timestamptz not null default now()
);
