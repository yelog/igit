#!/bin/sh
# A new disposable Push fixture: 11 roots, local bare remotes, 20 changed paths.
set -eu
here=$(CDPATH= cd "$(dirname "$0")" && pwd -P)
repo=$(sh "$here/demo-commit.sh")
base=${repo%/*}
mkdir -p "$base/remotes" "$base/seeds"

g() {
  env -i PATH="$PATH" LC_ALL=C GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
    GIT_AUTHOR_NAME='IGit Demo' GIT_AUTHOR_EMAIL='demo@igit.invalid' \
    GIT_COMMITTER_NAME='IGit Demo' GIT_COMMITTER_EMAIL='demo@igit.invalid' \
    GIT_AUTHOR_DATE='2026-11-03T10:00:00+00:00' GIT_COMMITTER_DATE='2026-11-03T10:00:00+00:00' \
    git -c commit.gpgSign=false -c core.hooksPath=/dev/null "$@"
}

# Submodules are added with a path-limited commit; the staged demo script stays pending.
for name in aps-hudong dcc devicemate eom maas mes mes-isg mes-mbg qds sep
do
  seed=$base/seeds/$name
  remote=$base/remotes/$name.git
  g init --bare -q -b master --template= "$remote"
  g init -q -b master --template= "$seed"
  printf '%s\n' "# $name demo" > "$seed/README.md"
  g -C "$seed" add -- README.md
  g -C "$seed" commit -qm "feat: initialize $name"
  g -C "$seed" remote add origin "$remote"
  g -C "$seed" push -qu origin master
  g -C "$repo" -c protocol.file.allow=always submodule add -q "$remote" "$name"
done
g -C "$repo" commit -qm 'build: track the demo submodules' --only -- \
  .gitmodules aps-hudong dcc devicemate eom maas mes mes-isg mes-mbg qds sep
g init --bare -q -b master --template= "$base/remotes/moss-web.git"
g -C "$repo" remote add origin "$base/remotes/moss-web.git"
g -C "$repo" push -qu origin master

mkdir -p "$repo/src/settings"
printf '%s\n' 'export const supportEnabled = true;' > "$repo/src/settings/siteSetting.ts"
g -C "$repo" add --all
g -C "$repo" commit -qm 'feat(customer-service): add PRC and ROW support'
printf '%s\n' "assert.equal('/support'.length, 8);" >> "$repo/tests/customerService.test.mjs"
g -C "$repo" commit -qam 'test(customer-service): cover the embedded support route'
for step in 1 2 3
do
  printf '%s\n' "Release note $step" >> "$repo/mes-isg/README.md"
  g -C "$repo/mes-isg" commit -qam "docs: add release note $step"
done
printf '%s\n' "$repo"
