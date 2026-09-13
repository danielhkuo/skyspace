#!/bin/sh
# Seed a development database from the parser fixtures, with no request to
# Rice: one Fall 2026 listing page, its terms list, one ASSOCIATED-SECTIONS
# XML and one detail page, registered as archived responses and then run
# through `skyspace replay`, exactly as a parser fix would be. Enough for the
# catalog, the class pane and a schedule; not a real term.
#
#   DATABASE_URL=postgres://... SKYSPACE_ARCHIVE_DIR=./archive sh scripts/seed-dev-db.sh
#
# Needs python3 and a way to run psql: PSQL defaults to `psql "$DATABASE_URL"`;
# with the README's container use PSQL="docker exec -i skyspace-pg psql -U postgres".
set -eu

: "${DATABASE_URL:?set DATABASE_URL}"
archive=${SKYSPACE_ARCHIVE_DIR:-./archive}
psql_cmd=${PSQL:-"psql $DATABASE_URL"}
skyspace=${SKYSPACE_BIN:-"cargo run -q -p skyspace-cli --"}
fixtures=crates/skyspace-parse/tests/fixtures
base='https://courses.rice.edu/courses/!SWKSCAT'
term=202710

# source | url | file | content type
rows=$(cat <<ROWS
reference|$base.info?action=TERMS|ref-terms.xml|text/xml; charset=utf-8
listing|$base.cat?p_action=QUERY&p_term=$term&p_subj=COMP|listing.html|text/html; charset=utf-8
section_xml|$base.info?action=ASSOCIATED-SECTIONS&crn=12312&term=$term|assoc.xml|text/xml; charset=utf-8
detail|$base.cat?p_action=COURSE&p_term=$term&p_crn=12422|detail.html|text/html; charset=utf-8
program|https://ga.rice.edu/programs-study/departments-programs/engineering/computer-science/computer-science-bscs/|ga-bscs.html|text/html; charset=utf-8
ROWS
)

# Write each fixture into the archive under its content hash (gzip, two-char
# prefix directory), and print the values the raw_responses row needs.
sql=$(printf '%s\n' "$rows" | python3 -c '
import gzip, hashlib, os, sys
archive, fixtures = sys.argv[1], sys.argv[2]
print("begin;")
print("insert into ingest_runs (job, term_code, started_at, finished_at, outcome) values (%s, %s, now(), now(), %s);" % ("\x27seed\x27", "\x27202710\x27", "\x27ok\x27"))
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    source, url, name, ctype = line.split("|")
    data = open(os.path.join(fixtures, name), "rb").read()
    key = hashlib.sha256(data).hexdigest()
    d = os.path.join(archive, key[:2]); os.makedirs(d, exist_ok=True)
    path = os.path.join(d, key + ".gz")
    if not os.path.exists(path):
        with gzip.open(path, "wb") as f: f.write(data)
    term = "\x27202710\x27" if "term=202710" in url or "p_term=202710" in url else "null"
    print("insert into raw_responses (run_id, source, url, term_code, fetched_at, status, content_type, byte_len, sha256) values ((select max(id) from ingest_runs where job = \x27seed\x27), \x27%s\x27, \x27%s\x27, %s, now(), 200, \x27%s\x27, %d, decode(\x27%s\x27, \x27hex\x27));" % (source, url.replace("\x27", "\x27\x27"), term, ctype, len(data), key))
print("commit;")
' "$archive" "$fixtures")

printf '%s\n' "$sql" | $psql_cmd -q -v ON_ERROR_STOP=1
echo "seed: archived 5 fixtures under $archive"

since=$(date -u +%F)
export SKYSPACE_ARCHIVE_DIR="$archive"
for source in reference listing section_xml detail program; do
    $skyspace replay --source "$source" --since "$since"
done
$skyspace term set-current $term
echo "seed: done; $term is current. The BSCS draft is in the review queue:"
$skyspace review list
echo "seed: approve it with: skyspace review approve <id> --reviewer <you>"
