#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UPSTREAM_DIR="${ALEPHIUM_UPSTREAM_DIR:-${ROOT_DIR}/.upstream/alephium}"
OUTPUT_DIR="${M1_VECTOR_OUTPUT_DIR:-${ROOT_DIR}/tests/vectors/m1/generated}"
SOURCE_FILE="${ROOT_DIR}/tools/upstream/m1/RustNodeM1VectorGenerator.scala"
TARGET_FILE="${UPSTREAM_DIR}/tools/src/main/scala/org/alephium/tools/RustNodeM1VectorGenerator.scala"
EXPECTED_COMMIT="63bcf6998508025f9538f3377629fe9e66df6c53"

command -v git >/dev/null 2>&1 || { echo "git is required" >&2; exit 1; }
command -v java >/dev/null 2>&1 || { echo "Java 17+ is required" >&2; exit 1; }
command -v sbt >/dev/null 2>&1 || { echo "sbt is required" >&2; exit 1; }
command -v sha256sum >/dev/null 2>&1 || { echo "sha256sum is required" >&2; exit 1; }

"${ROOT_DIR}/scripts/fetch-upstream.sh"

actual_commit="$(git -C "${UPSTREAM_DIR}" rev-parse HEAD)"
if [[ "${actual_commit}" != "${EXPECTED_COMMIT}" ]]; then
  echo "wrong upstream commit: ${actual_commit}" >&2
  exit 1
fi

if [[ -n "$(git -C "${UPSTREAM_DIR}" status --porcelain --untracked-files=no)" ]]; then
  echo "pinned upstream has tracked local modifications; refusing vector generation" >&2
  exit 1
fi

mkdir -p "$(dirname "${TARGET_FILE}")"
rm -rf "${OUTPUT_DIR}"
mkdir -p "${OUTPUT_DIR}"
cp "${SOURCE_FILE}" "${TARGET_FILE}"

cleanup() {
  rm -f "${TARGET_FILE}"
}
trap cleanup EXIT INT TERM

export RUSTNODE_VECTOR_OUTPUT="${OUTPUT_DIR}"
(
  cd "${UPSTREAM_DIR}"
  sbt "tools/runMain org.alephium.tools.RustNodeM1VectorGenerator"
)

required=(
  manifest.tsv
  network-magic.hex
  wire-header.hex
  hello-payload.hex
  hello-frame.hex
  ping-payload.hex
  ping-frame.hex
  pong-payload.hex
  pong-frame.hex
  compact-signed-int.tsv
  compact-unsigned-u32.tsv
  decode-results.tsv
  truncated-hello.tsv
)

for file in "${required[@]}"; do
  if [[ ! -s "${OUTPUT_DIR}/${file}" ]]; then
    echo "missing generated vector: ${file}" >&2
    exit 1
  fi
done

grep -Fq "${EXPECTED_COMMIT}" "${OUTPUT_DIR}/manifest.tsv"

(
  cd "${OUTPUT_DIR}"
  find . -type f ! -name SHA256SUMS -print0 \
    | sort -z \
    | xargs -0 sha256sum > SHA256SUMS
)

if [[ -n "$(git -C "${UPSTREAM_DIR}" status --porcelain --untracked-files=no)" ]]; then
  echo "vector generation modified tracked upstream files" >&2
  exit 1
fi

echo "Generated official Scala M1 vectors: ${OUTPUT_DIR}"
