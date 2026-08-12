#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STATE_DIR="${OFFICIAL_NODE_STATE_DIR:-${ROOT_DIR}/.local/official-node}"
HOME_DIR="${OFFICIAL_NODE_HOME:-${STATE_DIR}/home}"
JAR_PATH_FILE="${STATE_DIR}/jar-path"
JAVA_CMD="${JAVA_CMD:-java}"

if [[ ! -f "${JAR_PATH_FILE}" ]]; then
  echo "official node JAR is not built; run make official-build" >&2
  exit 1
fi

"${ROOT_DIR}/scripts/official-node/prepare-home.sh"

JAR_PATH="$(cat "${JAR_PATH_FILE}")"
if [[ ! -f "${JAR_PATH}" ]]; then
  echo "official node JAR no longer exists: ${JAR_PATH}" >&2
  exit 1
fi

if command -v "${JAVA_CMD}" >/dev/null 2>&1; then
  :
elif command -v "${JAVA_CMD}.exe" >/dev/null 2>&1; then
  JAVA_CMD="${JAVA_CMD}.exe"
else
  echo "Java not found (${JAVA_CMD})" >&2
  exit 1
fi

export ALEPHIUM_HOME="${HOME_DIR}"
export ALEPHIUM_ENV=prod

RUNNER_JAR_PATH="${JAR_PATH}"
RUNNER_HOME="${HOME_DIR}"
RUNNER_JAVA_CMD="${JAVA_CMD}"

if [[ "${JAVA_CMD}" == *.exe && -x /usr/bin/wslpath ]]; then
  RUNNER_JAR_PATH="$(wslpath -w "${JAR_PATH}")"
  RUNNER_HOME="$(wslpath -w "${HOME_DIR}")"
  RUNNER_JAVA_CMD="$(wslpath -w "${JAVA_CMD}")"
fi

if [[ "${RUNNER_JAVA_CMD}" == *" "* ]]; then
  RUNNER_JAVA_WRAPPER="${STATE_DIR}/official-node-java.bat"
  printf '@echo off\r\n"%s" %%*\r\n' "${RUNNER_JAVA_CMD}" > "${RUNNER_JAVA_WRAPPER}"
  RUNNER_JAVA_CMD="$(wslpath -w "${RUNNER_JAVA_WRAPPER}")"
fi

export ALEPHIUM_HOME="${RUNNER_HOME}"
export ALEPHIUM_ENV=prod

if [[ "${JAVA_CMD}" == *.exe ]] && [[ -x /mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe ]]; then
  powershell_script="$(mktemp /tmp/official-node-run-ps1.XXXXXX.ps1)"
  printf '$ErrorActionPreference = "Stop"\n' > "${powershell_script}"
  printf '$env:ALEPHIUM_HOME = "%s"\n' "${RUNNER_HOME}" >> "${powershell_script}"
  printf '$env:ALEPHIUM_ENV = "%s"\n' "prod" >> "${powershell_script}"
  printf '& "%s" -jar "%s"\n' "${RUNNER_JAVA_CMD}" "${RUNNER_JAR_PATH}" >> "${powershell_script}"
  powershell_script_windows="$(wslpath -w "${powershell_script}")"
  /mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "${powershell_script_windows}"
  rm -f "${powershell_script}"
  exit 0
fi

exec "${RUNNER_JAVA_CMD}" ${OFFICIAL_NODE_JAVA_OPTS:-} -jar "${RUNNER_JAR_PATH}"
