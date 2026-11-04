#!/bin/sh
# An isolated, merge-rich Git Log fixture. All identities and remotes are local.
set -eu
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null
base=$(mktemp -d "${TMPDIR:-/tmp}/igit-log.XXXXXXXX")
ws=$base/workspace
web=$ws/atlas-web
api=$ws/atlas-worker
mkdir -p "$ws"
g(){ git -c core.hooksPath=/dev/null -c commit.gpgSign=false -c tag.gpgSign=false -C "$repo" "$@"; }
init(){
  mkdir -p "$repo"
  g init -q -b main
  g config user.name 'Rae Lin'
  g config user.email rae@example.invalid
  g config core.hooksPath /dev/null
  g config commit.gpgSign false
}
# Start at a fixed date so the example has a reproducible order.
stamp=1788780960
advance(){
  stamp=$((stamp+126000))
  GIT_AUTHOR_DATE="$stamp +0000"; GIT_COMMITTER_DATE=$GIT_AUTHOR_DATE
  export GIT_AUTHOR_DATE GIT_COMMITTER_DATE
}
change(){
  advance
  mkdir -p "$repo/$(dirname "$1")"
  printf '%s\n' "$2" >> "$repo/$1"
  g add -- "$1"
  g commit -qm "$2"
}
publish_fixture(){
  remote=$base/$(basename "$repo").git
  git -c core.hooksPath=/dev/null clone -q --bare "$repo" "$remote"
  g remote add origin "$remote"
  g -c protocol.file.allow=always fetch -q origin
  g remote set-head origin main
  g for-each-ref --format='%(refname:strip=2)' refs/heads | while IFS= read -r branch
  do
    g branch -q --set-upstream-to="origin/$branch" "$branch"
  done
}
repo=$web
init
change README.md 'docs: document the workspace'
change src/views/system/user/index.vue 'feat(user): introduce the account view'
g branch develop
cycle(){
  name=$1; subject=$2; main_msg=$3; extra=$4
  g switch -qc "feat/$name" main
  change "src/features/$name/view.ts" "feat($name): $subject"
  change "src/features/$name/tests.ts" "test($name): cover $extra"
  g switch -q main
  change src/views/system/user/index.vue "$main_msg"
  advance; g merge -q --no-ff -m "Merge branch 'feat/$name' into main" "feat/$name"
  g switch -q develop
  advance; g merge -q --no-ff -m "Merge branch 'main' into develop" main
  g switch -q main
}
cycle access 'show effective permissions' 'fix(user): respect account restrictions' 'inherited roles'
cycle search 'index labels and descriptions' 'fix(search): preserve the active filter' 'mixed-language queries'
cycle export 'add CSV downloads' 'feat(log): add an export action' 'large reports'
cycle profile 'include recovery contacts' 'fix(profile): validate the contact field' 'invalid addresses'
cycle sessions 'list recent sign-ins' 'fix(auth): renew expiring sessions' 'clock changes'
cycle audit 'display account changes' 'fix(audit): keep records in event order' 'concurrent updates'
cycle settings 'edit notification rules' 'feat(user): persist notification settings' 'empty rules'
g branch fix/session-cache main~3
g branch access-review feat/access~1
g branch report-export feat/export~1
g branch session-history feat/sessions~1
g branch audit-review feat/audit~1
g branch notification-rules feat/settings~1
g branch integration develop~1
g branch release/1.4 main~2
g tag -a v1.4.0 -m 'Release 1.4' main~2
publish_fixture
change src/views/system/user/index.vue 'fix(user): restrict password reset to account owners'
change src/config/runtime.ts 'fix(runtime): preserve the deployment prefix in requests'
repo=$api
init
change README.md 'docs: describe the API'
change src/api/routes.rs 'feat(api): add the account endpoint'
g branch develop
g branch feat/search
publish_fixture
printf '%s\n' "$ws"
