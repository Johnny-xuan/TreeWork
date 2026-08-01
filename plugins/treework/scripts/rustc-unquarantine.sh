#!/usr/bin/env bash
set -eo pipefail

if [[ "$#" -lt 1 ]]; then
  echo "TreeWork rustc wrapper expected the rustc executable as its first argument." >&2
  exit 2
fi

rustc="$1"
shift
rustc_args=("$@")
crate_types=""
crate_name=""
extra_filename=""
out_dir=""
explicit_output=""

for ((index = 0; index < ${#rustc_args[@]}; index++)); do
  arg="${rustc_args[$index]}"
  case "$arg" in
    --crate-name)
      if ((index + 1 < ${#rustc_args[@]})); then
        crate_name="${rustc_args[$((index + 1))]}"
      fi
      ;;
    --crate-name=*)
      crate_name="${arg#--crate-name=}"
      ;;
    --crate-type)
      if ((index + 1 < ${#rustc_args[@]})); then
        crate_types="${rustc_args[$((index + 1))]}"
      fi
      ;;
    --crate-type=*)
      crate_types="${arg#--crate-type=}"
      ;;
    --out-dir)
      if ((index + 1 < ${#rustc_args[@]})); then
        out_dir="${rustc_args[$((index + 1))]}"
      fi
      ;;
    --out-dir=*)
      out_dir="${arg#--out-dir=}"
      ;;
    -o)
      if ((index + 1 < ${#rustc_args[@]})); then
        explicit_output="${rustc_args[$((index + 1))]}"
      fi
      ;;
    -o=*)
      explicit_output="${arg#-o=}"
      ;;
    -o?*)
      explicit_output="${arg#-o}"
      ;;
    -C)
      if ((index + 1 < ${#rustc_args[@]})); then
        codegen_option="${rustc_args[$((index + 1))]}"
        if [[ "$codegen_option" == extra-filename=* ]]; then
          extra_filename="${codegen_option#extra-filename=}"
        fi
      fi
      ;;
    -Cextra-filename=*)
      extra_filename="${arg#-Cextra-filename=}"
      ;;
  esac
done

if [[ -n "${TREEWORK_INNER_RUSTC_WRAPPER:-}" ]]; then
  "$TREEWORK_INNER_RUSTC_WRAPPER" "$rustc" "${rustc_args[@]}"
else
  "$rustc" "${rustc_args[@]}"
fi

if [[ -z "${TREEWORK_XATTR_BIN:-}" ]]; then
  echo "TreeWork rustc wrapper requires TREEWORK_XATTR_BIN on macOS." >&2
  exit 1
fi

artifact=""
if [[ -n "$explicit_output" ]]; then
  artifact="$explicit_output"
elif [[ -n "$out_dir" && -n "$crate_name" ]]; then
  case ",$crate_types," in
    *,bin,*)
      artifact="$out_dir/$crate_name$extra_filename"
      ;;
    *,proc-macro,*|*,dylib,*|*,cdylib,*)
      artifact="$out_dir/lib$crate_name$extra_filename.dylib"
      ;;
  esac
fi

if [[ -n "$artifact" && -e "$artifact" ]] \
  && "$TREEWORK_XATTR_BIN" -p com.apple.quarantine "$artifact" >/dev/null 2>&1; then
  "$TREEWORK_XATTR_BIN" -d com.apple.quarantine "$artifact"
fi
