# Deploying Skyspace

The whole stack on one Docker host: Caddy (TLS, the browser app, `/api`
proxy), the Axum API, Postgres, and a cron container that stands in for the
systemd timers of `docs/tech/05-ingest.md`. Cheap and temporary by design:
the smallest VPS with Docker preinstalled, one `compose up`, and `docker
compose down -v` plus deleting the VPS ends it.

| Service | Image | Runs |
|---|---|---|
| `web` | `deploy/web.Dockerfile` (Node build → Caddy) | Caddy on 80/443: certificate, static app, `/api/*` and `/health` to the API |
| `api` | `deploy/api.Dockerfile` | `skyspace-api` on 8080, unprivileged, behind Caddy only |
| `migrate` | same image | `skyspace migrate` once per `up`; the API waits for it |
| `jobs` | same image | cron with `deploy/jobs/crontab`: seats every 15 min, listings nightly, sections and detail weekly, reference weekly, catalog monthly, `doctor` hourly |
| `db` | `postgres:16-alpine` | the one datastore, in the `db` volume |

Volumes: `db` (Postgres), `archive` (every raw Rice response the jobs
fetch), `caddy_data` (certificates).

## First deploy

1. **A host.** Hetzner CX22 (about €4 a month) or DigitalOcean's smallest
   droplet, with the Docker image. Open ports 80 and 443 only.
2. **DNS.** In GoDaddy, an `A` record for the hostname (`@` or `skyspace`)
   pointing at the host's IP, TTL 600. Caddy needs the name to resolve
   before it can get a certificate.
3. **Configure.**

   ```bash
   git clone <repo> skyspace && cd skyspace
   cp deploy/.env.template deploy/.env
   $EDITOR deploy/.env     # SKYSPACE_DOMAIN, POSTGRES_PASSWORD, SKYSPACE_CONTACT_EMAIL
   ```

   `deploy/.env` is gitignored. Leave unused variables commented out, not
   empty: the API treats an empty `SKYSPACE_SMTP_HOST` as "SMTP configured".
4. **Start.**

   ```bash
   docker compose -f deploy/compose.yml up -d --build
   ```

   The first build compiles the workspace in release mode; expect ten
   minutes on a small VPS. Later builds reuse the cargo cache.
5. **Check.** `https://<domain>/health` answers from the API through Caddy,
   and `https://<domain>/` serves the app.

## Loading data

The database starts empty. Every command below runs the operator CLI
inside the jobs container, which already has the database and archive
wired up. `sj` is just a shorthand for this document:

```bash
sj() { docker compose -f deploy/compose.yml exec jobs "$@"; }
```

```bash
sj skyspace import university --file /srv/data/university/2026.toml
sj skyspace review list                         # note the draft id
sj skyspace review approve <id> --reviewer <you>
sj skyspace term set-current 202710
sj skyspace-job reference                       # ~9 requests
sj skyspace-job listings                        # ~96 requests, one per subject
sj skyspace-job catalog                         # ~106 requests, course records and prerequisites
sj skyspace-job sections                        # one per CRN, up to ~5,000: takes a while
sj skyspace-job detail                          # same, 1.4 s each: run overnight
sj skyspace-job requirements --only computer-science-bscs   # then review and approve, as above
sj skyspace doctor
```

`skyspace-job` fills in `--term` from `SKYSPACE_TERM` and derives both
catalog-year flags from it, so the runbook and the crontab name jobs, not
flags. Every pull identifies itself with `SKYSPACE_CONTACT_EMAIL`, waits at
least 150 ms between requests, and refuses to run without a contact address.

**Seats.** The 15-minute seats job exits at once unless a poll window is
open for the term (`docs/tech/05-ingest.md`). Windows are rows, not timers.
There is no CLI command for them yet, so open one with SQL:

```bash
docker compose -f deploy/compose.yml exec db psql -U skyspace -d skyspace -c \
  "insert into poll_windows (term_code, label, starts_at, ends_at, interval_minutes)
   values ('202710', 'Fall 2026 add/drop', now(), now() + interval '30 days', 15)"
```

## Sign-in on a staging box

Without SMTP nothing is mailed. The template sets `SKYSPACE_LOG_JSON=0` and
`SKYSPACE_LOG_LOGIN_CODES=1`, so each six-digit code appears in
`docker compose -f deploy/compose.yml logs api` beside a redacted address.
Production flips to `SKYSPACE_LOG_JSON=1`, which also makes it impossible
for a code to reach the log, and fills in the SMTP block.

## Day two

```bash
docker compose -f deploy/compose.yml logs -f api jobs      # what is happening
docker compose -f deploy/compose.yml exec jobs skyspace doctor   # config, database, age of every job's last good run
docker compose -f deploy/compose.yml exec db pg_dump -U skyspace skyspace | gzip > skyspace-$(date +%F).sql.gz   # backup
git pull && docker compose -f deploy/compose.yml up -d --build   # upgrade: migrate runs again, then the API restarts
docker compose -f deploy/compose.yml down -v                     # the end: drops the volumes too
```

A laptop run works the same way with `SKYSPACE_DOMAIN=localhost`; Caddy
issues its own certificate, which the browser will warn about once.

## What this deploy does not do yet

- **The browser app runs on demo data.** `web/src/datasource` has only the
  demo source; `VITE_DATA_SOURCE=api` is refused at build time until the API
  source exists. The backend is live and reachable at `/api/v1/*` and
  `/health` regardless, so the two can be finished independently.
- **One database role.** `docs/tech/05-ingest.md` wants the API on a
  `skyspace_app` role with append-only grants; here everything connects as
  the owner. The grants are a commented block at the end of migration 0008.
- **No alerting.** systemd's `OnFailure=` was the alerting plan; cron only
  logs. `doctor` runs hourly and exits 2 when a job is overdue, which is the
  line to watch until something pages.
- **No Rice SSO.** The API accepts SSO headers only from a loopback proxy,
  which this layout does not have. Sign-in is the emailed code.
- **Two ports on the host, one replica of everything.** Good for a semester
  of evaluation, not for a registration-week spike.
