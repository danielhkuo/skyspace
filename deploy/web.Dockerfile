# syntax=docker/dockerfile:1.7
# The browser app, built once and served by Caddy, which also terminates TLS
# and proxies /api to the backend (deploy/Caddyfile). Build context is the
# repository root.

FROM node:22-alpine AS build
WORKDIR /app
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web ./
# `demo` until the API data source exists in web/src/datasource; then `api`.
ARG VITE_DATA_SOURCE=demo
ENV VITE_DATA_SOURCE=$VITE_DATA_SOURCE
RUN npm run build

FROM caddy:2-alpine
COPY deploy/Caddyfile /etc/caddy/Caddyfile
COPY --from=build /app/dist /srv
