#!/bin/bash

set -ueo pipefail

id

GRP_PARENT=$(cat /proc/self/cgroup | tail -c+4)

echo $GRP_PARENT

GRP_NAME="isolate"

GRP_ROOT="/sys/fs/cgroup/$GRP_PARENT"

echo "Root: $GRP_ROOT"

#echo "+cpu +memory" > "$GRP_ROOT/cgroup.subtree_control"

FULL_GRP="/sys/fs/cgroup/$GRP_PARENT/$GRP_NAME"

sed -i -e "s|^cg_root.*|cg_root = $FULL_GRP|g" /usr/local/etc/isolate

mkdir $FULL_GRP

cat /proc/mounts | grep cgroup

ls -lah $FULL_GRP

/usr/local/bin/isolate-cg-keeper

./evaluator
