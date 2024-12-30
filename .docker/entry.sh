#!/bin/bash

set -ueo pipefail

CG_SELF=$(cat /proc/self/cgroup | tail -c+4)
CG_ROOT="/sys/fs/cgroup$CG_SELF"

ISOLATE_CG_FILE="/run/isolate/cgroup"

mkdir -p $(dirname $ISOLATE_CG_FILE) 

printf "$CG_ROOT" > "$ISOLATE_CG_FILE"

mkdir "$CG_ROOT/evaluator"
printf "$$" > "$CG_ROOT/evaluator/cgroup.procs"
printf "+cpuset +memory" > "$CG_ROOT/cgroup.subtree_control"

./docker/lang-versions.sh

exec ./evaluator
