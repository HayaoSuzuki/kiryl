#!/usr/bin/env bash
# Copyright (c) 2026 kiryl contributors
# Run one unprivileged CI command beneath a dedicated cgroup-v2 delegation.
# Never enable controllers on a shared ancestor or move the calling runner shell.
set -euo pipefail

if (( $# == 0 )); then
  echo 'usage: with-delegated-cgroup.sh COMMAND [ARGUMENT...]' >&2
  exit 2
fi
cgroup_mount=$(findmnt -n -t cgroup2 -o TARGET)
test -n "$cgroup_mount"
test "$(wc -l <<< "$cgroup_mount")" -eq 1
test -f "$cgroup_mount/cgroup.subtree_control"
if ! tr ' ' '\n' < "$cgroup_mount/cgroup.subtree_control" | grep -qx memory; then
  echo 'Shared cgroup root does not distribute memory; delegation unavailable' >&2
  exit 1
fi
delegated="$cgroup_mount/kiryl-ci-${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-0}-${RANDOM}"
sudo mkdir "$delegated"
cleanup() {
  sudo rmdir "$delegated/controller" "$delegated"
}
trap cleanup EXIT
echo +memory | sudo tee "$delegated/cgroup.subtree_control" >/dev/null
if [[ -f "$delegated/memory.swap.max" ]]; then
  echo 0 | sudo tee "$delegated/memory.swap.max" >/dev/null
fi
sudo mkdir "$delegated/controller"
# Only this owned subtree is delegated to the runner. This also tests the
# backend with ordinary user permissions, rather than running product tests as root.
sudo chown -R -- "$(id -u):$(id -g)" "$delegated"
sudo bash -c '
  set -euo pipefail
  delegation=$1
  runner_user=$2
  runner_path=$3
  runner_libraries=$4
  shift 4
  echo $$ > "$delegation/controller/cgroup.procs"
  exec runuser -u "$runner_user" -- env PATH="$runner_path" \
    LD_LIBRARY_PATH="$runner_libraries" KIRYL_CGROUP_PARENT="$delegation" "$@"
' bash "$delegated" "$(id -un)" "$PATH" "${LD_LIBRARY_PATH:-}" "$@"
