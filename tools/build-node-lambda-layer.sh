#!/usr/bin/env bash

set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "usage: $0 <layer-dir> [zip-output]" >&2
  exit 1
fi

layer_dir="$1"
zip_output="${2:-}"

package_name="${XIFTY_NODE_PACKAGE:-@xifty/xifty}"
package_version="${XIFTY_NODE_VERSION:-0.1.2}"
package_spec="${package_name}@${package_version}"

runtime_root="${layer_dir}/nodejs"
package_json="${runtime_root}/package.json"

rm -rf "${runtime_root}"
mkdir -p "${runtime_root}"

cat > "${package_json}" <<EOF
{
  "name": "xifty-lambda-layer",
  "private": true,
  "description": "Lambda layer assembly for ${package_spec}"
}
EOF

npm install --omit=dev --prefix "${runtime_root}" "${package_spec}"

prebuild_root="${runtime_root}/node_modules/@xifty/xifty/prebuilds"
if [[ -d "${prebuild_root}" ]]; then
  find "${prebuild_root}" -mindepth 1 -maxdepth 1 ! -name "linux-x64" -exec rm -rf {} +
fi

find "${runtime_root}" -name ".DS_Store" -delete

if [[ -n "${zip_output}" ]]; then
  zip_output="$(cd "$(dirname "${zip_output}")" && pwd)/$(basename "${zip_output}")"
  mkdir -p "$(dirname "${zip_output}")"
  rm -f "${zip_output}"
  (
    cd "${layer_dir}"
    zip -qr "${zip_output}" nodejs
  )
  echo "Built Lambda layer zip at ${zip_output}"
else
  echo "Prepared Lambda layer contents at ${layer_dir}"
fi
