# syntax=docker/dockerfile:1

# ---------------------------------------------------------------------------
# Build stage: Quasar SPA
# ---------------------------------------------------------------------------
FROM docker.io/library/node:26-alpine AS build

ENV CI=true \
    PNPM_HOME=/pnpm \
    PATH=/pnpm:$PATH

RUN npm install --global pnpm@10

WORKDIR /app

# Dependencies first so source changes do not invalidate the install layer. Lifecycle scripts are
# skipped here because the project's postinstall (`quasar prepare`) needs the sources.
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile --ignore-scripts

COPY . .

RUN pnpm exec quasar prepare \
 && pnpm exec quasar build \
 && rm -rf node_modules

# ---------------------------------------------------------------------------
# Runtime stage: nginx (unprivileged, UID 101, port 8080)
# ---------------------------------------------------------------------------
FROM docker.io/nginxinc/nginx-unprivileged:stable-alpine

# ${BACKEND_URL} is substituted into the template by the image's entrypoint
# (only variables matching NGINX_ENVSUBST_FILTER are replaced).
ENV BACKEND_URL=http://backend:8080 \
    NGINX_ENVSUBST_FILTER=^BACKEND_

COPY --from=build /app/dist/spa /usr/share/nginx/html
COPY nginx/security-headers.conf /etc/nginx/snippets/security-headers.conf
COPY nginx/default.conf.template /etc/nginx/templates/default.conf.template

EXPOSE 8080
