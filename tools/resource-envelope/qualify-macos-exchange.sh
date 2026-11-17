#!/bin/bash
set -euo pipefail

fixture=${1:?pass the compiled compound fixture}
source=${2:?pass the fixture source ID}
base=${3:?pass a fresh output directory}
mkdir "$base"

for attempt in 1 2 3; do
  output="$base/attempt-$attempt"
  if cargo xtask macos-maintenance-space --namespace-enospc \
    --fixture "$fixture" --source "$source" --output "$output"; then
    exit 0
  fi

  # Retry only after the kernel allowed exchange and the complete published
  # state, next commit and volume cleanup all verified.
  jq -e --arg source "$source" '
    .source_id == $source and .verified == false and .volume_cleanup.verified == true
    and .inventory_failure == null
    and .failure == "APFS exchange completed at capacity without the required kernel ENOSPC"
    and .exchange_completed_at_capacity.advanced_with_ballast == true
    and .exchange_completed_at_capacity.scratch_absent_after_remount == true
  ' "$output/evidence.json" >/dev/null
done
exit 1
