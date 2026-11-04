#!/bin/sh
# Create only a NEW temporary directory. Git remotes point to local bare repositories.
set -eu
base=$(mktemp -d "${TMPDIR:-/tmp}/igit-demo.XXXXXXXX")
ws=$base/workspace
mkdir -p "$ws" "$base/remotes"
export GIT_AUTHOR_NAME='IGit Demo' GIT_AUTHOR_EMAIL='demo@igit.invalid'
export GIT_COMMITTER_NAME="$GIT_AUTHOR_NAME" GIT_COMMITTER_EMAIL="$GIT_AUTHOR_EMAIL"
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR
demo_git(){ git -c commit.gpgSign=false -c tag.gpgSign=false -c core.hooksPath=/dev/null "$@"; }
init(){ demo_git init -q -b main "$1"; demo_git -C "$1" config user.name "$GIT_AUTHOR_NAME"; demo_git -C "$1" config user.email "$GIT_AUTHOR_EMAIL"; demo_git -C "$1" config commit.gpgSign false; demo_git -C "$1" config core.hooksPath /dev/null; }
for repo in sdk-core atlas-web atlas-api
do
  demo_git init --bare -q -b main "$base/remotes/$repo.git"
  if [ "$repo" = sdk-core ]; then dest="$base/sdk-source"; else dest="$ws/$repo"; fi
  init "$dest"
  mkdir -p "$dest/src"
  echo "$repo" > "$dest/README.md"
  echo "timeout = 10" > "$dest/src/config.txt"
  demo_git -C "$dest" add -- README.md src/config.txt
  demo_git -C "$dest" commit -qm "feat: initialize $repo"
  demo_git -C "$dest" remote add origin "$base/remotes/$repo.git"
  demo_git -C "$dest" push -qu origin main
done
web=$ws/atlas-web
api=$ws/atlas-api
demo_git -C "$web" -c protocol.file.allow=always submodule add -q "$base/remotes/sdk-core.git" deps/sdk-core
demo_git -C "$web" commit -qm 'build: track shared SDK'
demo_git -C "$web" push -q origin main
sdk=$web/deps/sdk-core
demo_git -C "$sdk" config user.name "$GIT_AUTHOR_NAME"
demo_git -C "$sdk" config user.email "$GIT_AUTHOR_EMAIL"
demo_git -C "$sdk" config commit.gpgSign false
demo_git -C "$sdk" config core.hooksPath /dev/null
# One genuine Git conflict plus independent changes on both sides.
cat > "$sdk/transport.conf" <<'DATA'
path = /api
a
b
c
transport = http
d
e
f
retries = 2
DATA
demo_git -C "$sdk" add -- transport.conf
demo_git -C "$sdk" commit -qm 'feat: configure transport'
demo_git -C "$sdk" switch -qc incoming
sed -e 's/transport = http/transport = http2/' -e 's/retries = 2/retries = 5/' "$sdk/transport.conf" > "$base/edit"
cp "$base/edit" "$sdk/transport.conf"
demo_git -C "$sdk" commit -qam 'feat: enable http2 and retries'
demo_git -C "$sdk" switch -q main
sed -e 's|path = /api|path = /api/v2|' -e 's/transport = http/transport = websocket/' "$sdk/transport.conf" > "$base/edit"
cp "$base/edit" "$sdk/transport.conf"
demo_git -C "$sdk" commit -qam 'feat: configure websocket transport'
# Pin the parent to an already published child commit before producing the conflict.
demo_git -C "$sdk" push -q origin main
demo_git -C "$web" add -- deps/sdk-core
demo_git -C "$web" commit -qm 'build: pin the public SDK revision'
demo_git -C "$web" push -q origin main
# Two normal roots each have an outgoing commit, a tracked edit and an untracked file.
for dest in "$web" "$api"
do
  echo 'Release notes' >> "$dest/README.md"
  demo_git -C "$dest" commit -qam 'docs: add release notes'
  echo 'timeout = 30' > "$dest/src/config.txt"
  echo 'A draft, not selected by default' > "$dest/notes.txt"
done
if demo_git -C "$sdk" merge --no-edit incoming >"$base/merge.log" 2>&1; then echo 'Expected a conflict' >&2; exit 1; fi
test -n "$(demo_git -C "$sdk" ls-files -u)"
echo "$ws"
