#!/bin/bash
# OOM Trigger Script
# Allocates memory until the container's cgroup limit triggers an OOM kill.
#
# Usage:
#   docker run --memory=64m aion/oom-trigger
#   kubectl run oom-test --image=aion/oom-trigger --limits=memory=64Mi --restart=Never

set -e

ALLOC_MB="${OOM_ALLOC_MB:-512}"
DELAY_SECS="${OOM_DELAY_SECS:-5}"

echo "=== AION OOM Trigger ==="
echo "Target allocation: ${ALLOC_MB}MB"
echo "Delay before trigger: ${DELAY_SECS}s"
echo "Container memory limit: $(cat /sys/fs/cgroup/memory.max 2>/dev/null || echo 'unknown')"
echo ""

echo "Waiting ${DELAY_SECS}s before triggering OOM..."
sleep "$DELAY_SECS"

echo "Allocating ${ALLOC_MB}MB of memory..."
stress-ng --vm 1 --vm-bytes "${ALLOC_MB}M" --vm-hang 0 --timeout 300s
