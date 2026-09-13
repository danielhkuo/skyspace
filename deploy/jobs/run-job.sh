#!/bin/sh
# One named job, with the term and years filled in from the environment, so
# the crontab and the runbook say `skyspace-job seats` instead of repeating
# flags. Rice's six-digit term code carries the academic year (202710 is
# 2027, Fall 2026); the CATALIST catalog pull takes that year and the GA
# requirements pull takes the year the announcements are named for, one less.
set -eu

# cron starts with an empty environment; the entrypoint saved the container's.
if [ -f /etc/skyspace/env ]; then
    set -a
    . /etc/skyspace/env
    set +a
fi

term=${SKYSPACE_TERM:?SKYSPACE_TERM is not set (deploy/.env)}
academic_year=$(printf '%s' "$term" | cut -c1-4)
ga_year=$((academic_year - 1))
job=${1:?usage: skyspace-job <seats|listings|sections|detail|reference|catalog|requirements|doctor>}
shift

echo "skyspace-job: $job starting $(date -u +%FT%TZ)"
case "$job" in
    seats)        exec skyspace pull seats --term "$term" "$@" ;;
    listings)     exec skyspace pull listings --term "$term" "$@" ;;
    sections)     exec skyspace pull sections --term "$term" "$@" ;;
    detail)       exec skyspace pull detail --term "$term" "$@" ;;
    reference)    exec skyspace pull reference --term "$term" "$@" ;;
    catalog)      exec skyspace pull catalog --catalog-year "$academic_year" "$@" ;;
    requirements) exec skyspace pull requirements --catalog-year "$ga_year" "$@" ;;
    doctor)       exec skyspace doctor "$@" ;;
    *) echo "skyspace-job: unknown job $job" >&2; exit 64 ;;
esac
