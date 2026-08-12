#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
UPSTREAM_DIR="${ALEPHIUM_UPSTREAM_DIR:-${ROOT_DIR}/.upstream/alephium}"
STATE_DIR="${OFFICIAL_NODE_STATE_DIR:-${ROOT_DIR}/.local/official-node}"
UPSTREAM_CONFIG="${ROOT_DIR}/UPSTREAM.toml"

resolve_command() {
  local base_name="$1"
  if command -v "${base_name}" >/dev/null 2>&1; then
    printf '%s\n' "${base_name}"
    return 0
  fi
  if command -v "${base_name}.exe" >/dev/null 2>&1; then
    printf '%s\n' "${base_name}.exe"
    return 0
  fi
  return 1
}

GIT_CMD="${GIT_CMD:-$(resolve_command git || true)}"
JAVA_CMD="${JAVA_CMD:-$(resolve_command java || true)}"
SBT_CMD="${SBT_CMD:-$(resolve_command sbt || true)}"

if [[ -z "${GIT_CMD}" ]]; then
  echo "git is required" >&2
  exit 1
fi
if [[ -z "${JAVA_CMD}" ]]; then
  echo "Java 17+ is required" >&2
  exit 1
fi

PINNED_COMMIT="$(awk -F= '$1 ~ /^[[:space:]]*commit[[:space:]]*$/ {gsub(/[[:space:]"]/, "", $2); print $2; exit}' "${UPSTREAM_CONFIG}")"
PINNED_COMMIT_SHORT=""
if [[ -n "${PINNED_COMMIT}" ]]; then
  PINNED_COMMIT_SHORT="${PINNED_COMMIT:0:8}"
fi

if [[ -z "${SBT_CMD}" ]]; then
  if [[ -f "${STATE_DIR}/jar-path" ]]; then
    existing_jar="$(cat "${STATE_DIR}/jar-path" | tr -d '\r\n')"
    if [[ -z "${PINNED_COMMIT}" ]]; then
      echo "warning: unable to parse pinned commit from ${UPSTREAM_CONFIG}; reusing cached JAR at ${existing_jar}." >&2
      if [[ -f "${existing_jar}" ]]; then
        exit 0
      fi
    fi
    if [[ -f "${existing_jar}" ]] && [[ "${existing_jar}" == *"${PINNED_COMMIT}"* || "${existing_jar}" == *"${PINNED_COMMIT_SHORT}"* ]]; then
      echo "sbt is required only to rebuild official JAR; reusing existing cached JAR at ${existing_jar}."
      exit 0
    fi
  fi
  if [[ -n "${PINNED_COMMIT}" ]]; then
    echo "sbt is required to build official JAR for pinned commit ${PINNED_COMMIT}, and cached JAR is missing or stale." >&2
  else
    echo "sbt is required" >&2
  fi
  exit 1
fi

GIT_CMD="${GIT_CMD}" "${ROOT_DIR}/scripts/fetch-upstream.sh"

(
  cd "${UPSTREAM_DIR}"
  "${SBT_CMD}" app/assembly
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
