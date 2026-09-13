-- Sessions and the emailed sign-in code. Tokens and codes are stored as
-- SHA-256 only.

create table sessions (
    token_sha256 bytea       primary key,
    account_id   uuid        not null references accounts (id) on delete cascade,
    created_at   timestamptz not null default now(),
    expires_at   timestamptz not null,
    last_seen_at timestamptz not null default now()
);
create index sessions_account on sessions (account_id);
create index sessions_expiry  on sessions (expires_at);

-- One live code per address.
create table login_codes (
    email             text        primary key,
    code_sha256       bytea       not null,
    expires_at        timestamptz not null,
    attempts          smallint    not null default 0,
    sends_in_window   smallint    not null default 1,
    window_started_at timestamptz not null default now()
);

-- Rows older than a day are deleted nightly, and the privacy page says so.
create table login_ip_windows (
    ip                inet        primary key,
    sends             smallint    not null default 1,
    window_started_at timestamptz not null default now()
);
