#!/bin/bash

set -ueo pipefail

CG_SELF=$(grep "^0::" /proc/self/cgroup | cut -d: -f3-)
CG_ROOT="/sys/fs/cgroup$CG_SELF"

ISOLATE_CG_FILE="/run/isolate/cgroup"

mkdir -p "$(dirname "$ISOLATE_CG_FILE")"

printf '%s' "$CG_ROOT" > "$ISOLATE_CG_FILE"

mkdir -p "$CG_ROOT/evaluator"
printf '%s' "$$" > "$CG_ROOT/evaluator/cgroup.procs"

CONTROLLERS=""
for controller in cpuset cpu memory pids; do
    if grep -qw "$controller" "$CG_ROOT/cgroup.controllers"; then
        CONTROLLERS="$CONTROLLERS +$controller"
    fi
done
printf '%s' "$CONTROLLERS" > "$CG_ROOT/cgroup.subtree_control"

./docker/lang-versions.sh

exec /app/evaluator
