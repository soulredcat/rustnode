#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
UPSTREAM_DIR="${ALEPHIUM_UPSTREAM_DIR:-${ROOT_DIR}/.upstream/alephium}"
STATE_DIR="${OFFICIAL_NODE_STATE_DIR:-${ROOT_DIR}/.local/official-node}"

command -v git >/dev/null 2>&1 || { echo "git is required" >&2; exit 1; }
command -v java >/dev/null 2>&1 || { echo "Java 17+ is required" >&2; exit 1; }
command -v sbt >/dev/null 2>&1 || { echo "sbt is required" >&2; exit 1; }

"${ROOT_DIR}/scripts/fetch-upstream.sh"

(
  cd "${UPSTREAM_DIR}"
  sbt app/assembly
)

mapfile -t jars < <(
  find "${UPSTREAM_DIR}/app/target/scala-2.13" \
    -maxdepth 1 \
    -type f \
    -name 'alephium-*.jar' \
    ! -name '*sources*' \
    ! -name '*javadoc*' \
    -print | sort
)

if [[ "${#jars[@]}" -ne 1 ]]; then
  echo "expected exactly one official Alephium assembly JAR, found ${#jars[@]}" >&2
  printf 'candidate: %s\n' "${jars[@]:-<none>}" >&2
  exit 1
fi

mkdir -p "${STATE_DIR}"
printf '%s\n' "${jars[0]}" > "${STATE_DIR}/jar-path"
echo "Official Alephium JAR: ${jars[0]}"
