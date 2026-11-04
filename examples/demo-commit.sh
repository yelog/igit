#!/bin/sh
# A disposable single-root Commit fixture. Its 19 paths match the IDEA reference.
set -eu
base=$(mktemp -d "${TMPDIR:-/tmp}/igit-commit.XXXXXXXX")
repo=$(CDPATH= cd "$base" && pwd -P)/moss-web
mkdir "$repo"

# In particular, an inherited GIT_DIR or GIT_INDEX_FILE must not redirect these writes.
g() {
  env -i PATH="$PATH" LC_ALL=C GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
    GIT_AUTHOR_DATE='2026-11-03T09:00:00+00:00' GIT_COMMITTER_DATE='2026-11-03T09:00:00+00:00' \
    git -C "$repo" "$@"
}
g init -q -b master --template=
g config user.name 'IGit Demo'
g config user.email demo@igit.invalid
g config commit.gpgSign false
g config core.hooksPath /dev/null
g config core.excludesFile /dev/null

write() {
  target=$repo/$1
  shift
  mkdir -p "${target%/*}"
  printf '%s\n' "$@" > "$target"
}

tracked() {
  mode=$1
  write .env.development.example "VITE_SERVICE_MODE=$mode"
  write src/store/modules/app.ts "export const serviceMode = '$mode';"
  write src/App.vue "<template><main data-mode=\"$mode\">Demo workspace</main></template>"
  write types/global.d.ts "type ServiceMode = '$mode';"
  write src/layouts/default/customer-service/index.vue "<template><aside class=\"$mode\">Support</aside></template>"
  write src/views/system/ui-config/index.vue "<template><section>Service mode: $mode</section></template>"
  write package.json '{' '  "name": "moss-web-demo",' '  "private": true,' "  \"description\": \"$mode service example\"" '}'
  write src/locales/lang/en_US/system.ts "export const labels = { $mode: 'Help' };"
  write src/locales/lang/es_MX/system.ts "export const labels = { $mode: 'Ayuda' };"
  write src/locales/lang/zh_CN/system.ts "export const labels = { $mode: '帮助' };"
}
tracked inline
g add -- .
g commit -qm 'feat: add the sample service views'
tracked embedded

# AM: the staged script is the candidate; the second line exists only in the worktree.
write scripts/iframe-proxy.mjs "export const route = '/support';"
g add -- scripts/iframe-proxy.mjs
printf '%s\n' 'export const debug = true;' >> "$repo/scripts/iframe-proxy.mjs"

write docs/plans/2026-11-03-customer-service-modes.md '# Service modes' 'Compare the inline and embedded views.'
write docs/plans/2026-11-03-iframe-https-proxy.md '# Local preview' 'Outline the proxy route for the embedded view.'
write src/layouts/default/customer-service/aihub.ts "export const sampleReply = 'How can we help?';"
write dev/iframe-proxy/compose.yaml 'name: preview' 'services: {}'
write tests/customerService.test.mjs "import assert from 'node:assert/strict';" "assert.equal('/support'.startsWith('/'), true);"
write src/utils/customerService.ts "export const supportPath = '/support';"
write dev/iframe-proxy/nginx.conf 'events {}' 'http { server { listen 8080; } }'
write dev/iframe-proxy/README.md '# Embedded preview' 'Local configuration for the sample support view.'

printf '%s\n' "$repo"
