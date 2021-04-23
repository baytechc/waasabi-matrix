#!/usr/bin/env bash

# Updating the changelog to include the released version.

set -eo pipefail

WORKSPACE_ROOT="$( cd "$(dirname "$0")/.." ; pwd -P )"

if [ -z "$PREV_VERSION" ] || [ -z "$NEW_VERSION" ]; then
    echo "Missing PREV_VERSION or NEW_VERSION." >&2
    echo "This script needs to run as a 'pre-release-hook' from cargo-release." >&2
    exit 1
fi

DATE=$(date +%Y-%m-%d)

echo "Preparing update to v${NEW_VERSION} (${DATE})"
echo "Workspace root: ${WORKSPACE_ROOT}"

### CHANGELOG ###

FILE=CHANGELOG.md
sed -i.bak -E \
    -e "s/# Unreleased changes/# v${NEW_VERSION} (${DATE})/" \
    -e "s/\.\.\.main/...v${NEW_VERSION}/" \
    "${WORKSPACE_ROOT}/${FILE}"
rm "${WORKSPACE_ROOT}/${FILE}.bak"

CHANGELOG=$(cat "${WORKSPACE_ROOT}/${FILE}")
cat > "${WORKSPACE_ROOT}/${FILE}" <<EOL
# Unreleased changes

[Full changelog](https://github.com/baytechc/waasabi-matrix/compare/v${NEW_VERSION}...main)

${CHANGELOG}
EOL
