#!/bin/sh
# The jobs service: hand the container's environment to cron (which starts
# its children with none), then run cron in the foreground so the container
# lives and dies with it. Runs as root because cron requires it; the API
# container runs as the unprivileged `skyspace` user.
set -eu
mkdir -p /etc/skyspace
umask 077
# Quote every value so a password with spaces or `#` survives the round trip.
env | grep -E '^(DATABASE_URL|SKYSPACE_[A-Z_]+)=' \
    | sed "s/'/'\\\\''/g; s/^\([^=]*\)=\(.*\)$/\1='\2'/" > /etc/skyspace/env
echo "skyspace-cron: $(grep -c . /etc/skyspace/env) variables saved; schedule in /etc/cron.d/skyspace"
exec cron -f
