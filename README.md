# Skyspace

Course discovery and degree planning for Rice University students.

One web app that does three jobs:

1. **Find** courses offered in the current term.
2. **Fit** them into a schedule, and compare options.
3. **Plan** a whole degree against its requirements.

## Backend

Seven Rust crates in one workspace (`crates/`): `skyspace-core` (the pure
engine, also compiled to WebAssembly by `skyspace-wasm`), `skyspace-parse`
(Rice HTML and XML into core types), `skyspace-store` (every SQL statement),
`skyspace-ingest` (polite fetch, archive, jobs), `skyspace-api` (Axum) and
`skyspace-cli` (the `skyspace` operator binary). The design is in the local
`Docs/tech/` files.

Run the stack:

```bash
docker run -d --name skyspace-pg -e POSTGRES_PASSWORD=skyspace -p 55432:5432 postgres:16-alpine
export DATABASE_URL=postgres://postgres:skyspace@127.0.0.1:55432/postgres
cargo run -p skyspace-cli -- migrate
cargo run -p skyspace-cli -- import university --file data/university/2026.toml
cargo run -p skyspace-cli -- review approve 1 --reviewer you
cargo run -p skyspace-api
```

Pull jobs need `SKYSPACE_CONTACT_EMAIL` set to a real address before they
will touch `courses.rice.edu` or `ga.rice.edu`; see `skyspace --help`.

Checks: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --workspace` (needs `DATABASE_URL`), `sh ci/check-purity.sh`,
`wasm-pack test --node crates/skyspace-wasm`, `sh scripts/check-wasm-size.sh`.
Regenerate the TypeScript wire types with
`TS_RS_EXPORT_DIR=$PWD/web/src/api/generated cargo test -p skyspace-core -p skyspace-api --features ts`.

## Deploying

`deploy/compose.yml` runs the whole stack on one Docker host: Postgres,
`skyspace migrate` (once per start), the API, a cron container, and Caddy.
The only ports on the host are 80 and 443, both Caddy's: it holds the
certificate, serves the browser app, and proxies `/api/*` and `/health` to
the API, which is reachable from nowhere else. The cron container runs the
`skyspace` pull jobs against Rice on a schedule (`deploy/jobs/crontab`):
seats every 15 minutes inside a poll window, listings nightly, sections and
detail weekly, `doctor` hourly. Every pull identifies itself with
`SKYSPACE_CONTACT_EMAIL`, waits 150 ms between requests, and archives the
raw response in the `archive` volume. The browser app is still on demo data
until `web/src/datasource` gains an API source. Setup, data bootstrap and
day-two commands: `deploy/README.md`.
