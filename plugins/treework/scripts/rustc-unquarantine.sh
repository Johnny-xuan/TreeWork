#!/usr/bin/env bash
set -eo pipefail

if [[ "$#" -lt 1 ]]; then
  echo "TreeWork rustc wrapper expected the rustc executable as its first argument." >&2
  exit 2
fi

rustc="$1"
shift
rustc_args=("$@")
cleanup_targets=()
crate_types=""

for ((index = 0; index < ${#rustc_args[@]}; index++)); do
  arg="${rustc_args[$index]}"
  case "$arg" in
    --crate-type)
      if ((index + 1 < ${#rustc_args[@]})); then
        crate_types="${rustc_args[$((index + 1))]}"
      fi
      ;;
    --crate-type=*)
      crate_types="${arg#--crate-type=}"
      ;;
    --out-dir|-o)
      if ((index + 1 < ${#rustc_args[@]})); then
        cleanup_targets+=("${rustc_args[$((index + 1))]}")
      fi
      ;;
    --out-dir=*)
      cleanup_targets+=("${arg#--out-dir=}")
      ;;
    -o=*)
      cleanup_targets+=("${arg#-o=}")
      ;;
    -o?*)
      cleanup_targets+=("${arg#-o}")
      ;;
  esac
done

if [[ -n "${TREEWORK_INNER_RUSTC_WRAPPER:-}" ]]; then
  "$TREEWORK_INNER_RUSTC_WRAPPER" "$rustc" "${rustc_args[@]}"
else
  "$rustc" "${rustc_args[@]}"
fi

xattr_bin="${TREEWORK_XATTR_BIN:-}"
if [[ -z "$xattr_bin" ]]; then
  echo "TreeWork rustc wrapper requires TREEWORK_XATTR_BIN on macOS." >&2
  exit 1
fi

case ",$crate_types," in
  *,bin,*|*,proc-macro,*|*,dylib,*|*,cdylib,*)
    for target in "${cleanup_targets[@]}"; do
      if [[ -e "$target" ]]; then
        "$xattr_bin" -dr com.apple.quarantine "$target"
      fi
    done
    ;;
esac
