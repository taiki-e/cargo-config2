#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
# shellcheck disable=SC2207
set -CeEuo pipefail
IFS=$'\n\t'
trap -- 's=$?; printf >&2 "%s\n" "${0##*/}:${LINENO}: \`${BASH_COMMAND}\` exit with ${s}"; exit ${s}' ERR
cd -- "$(dirname -- "$0")"/..

# Generates code based on target-spec.
#
# USAGE:
#    ./tools/target_spec.sh
#
# This script is intended to be called by gen.sh, but can be called separately.

file=src/gen/cfg.rs
mkdir -p -- "$(dirname -- "${file}")"
mkdir -p -- tools/gen/cfg

bail() {
  printf >&2 'error: %s\n' "$*"
  exit 1
}

host=$(rustc -vV | grep -E '^host:' | cut -d' ' -f2)
# --print cfg is available on Rust 1.8+.
start=8
if [[ "${host}" == 'x86_64-unknown-linux-gnu' ]]; then
  print_cfg() {
    local toolchain="$1"
    printf >&2 '%s:\n' "${toolchain}"
    for target in $(rustc +"${toolchain}" --print target-list); do
      # Handle error cases.
      case "${target}" in
        *-apple-ios) case "${toolchain}" in 1.[89] | 1.1[01]) continue ;; esac ;;
        asmjs-unknown-emscripten) case "${toolchain}" in 1.1[0-3]) continue ;; esac ;;
        le32-unknown-nacl) case "${toolchain}" in 1.1[0-9] | 1.2[01]) continue ;; esac ;;
        wasm32-experimental-emscripten) case "${toolchain}" in 1.2[0-3]) continue ;; esac ;;
        wasm32-unknown-unknown) case "${toolchain}" in 1.23) continue ;; esac ;;
        aarch64-apple-watchos) case "${toolchain}" in 1.76) continue ;; esac ;;
      esac
      printf >&2 '%s\n' "${target}"
      printf '%s:\n' "${target}"
      rustc +"${toolchain}" --print cfg --target "${target}" | { grep -Ev '^(debug_assertions|overflow_checks|ub_checks)$' || true; }
      printf '\n'
    done
  }
  if [[ -n "${GITHUB_ACTIONS:-}" ]]; then
    # Handle outdated toolchain in CI runner.
    rustup toolchain remove stable
  fi
  stable=$(rustc +stable -vV | grep -E '^release:' | cut -d' ' -f2)
  stable="${stable#*.}"
  stable="${stable%%.*}"
  printf '%s\n' "1.${stable}" >|tools/gen/cfg/stable.txt
  for minor in $(seq "${start}" "$((stable - 1))"); do
    cfg=$(print_cfg "1.${minor}")
    printf '%s\n' "${cfg}" >|tools/gen/cfg/"1.${minor}".txt
  done
  cfg=$(print_cfg stable)
  printf '%s\n' "${cfg}" >|tools/gen/cfg/"1.${stable}".txt
  cfg=$(print_cfg beta)
  printf '%s\n' "${cfg}" >|tools/gen/cfg/beta.txt
  cfg=$(print_cfg nightly)
  printf '%s\n' "${cfg}" >|tools/gen/cfg/nightly.txt
  stable="1.${stable}"
else
  stable=$(<tools/gen/cfg/stable.txt)
fi
start="1.${start}"

enum() {
  local cfg_name="$1"
  local name
  name=$(sed -E 's/(^|-|_)(\w)/\U\2/g' <<<"${cfg_name}")
  shift
  if [[ -z "${EXHAUSTIVE:-}" ]] && [[ -z "${NUM:-}" ]]; then
    local error='core::convert::Infallible'
  else
    local error='crate::Error'
  fi
  cat <<EOF
/// \`cfg(${cfg_name} == "..")\`
EOF
  case "${cfg_name}" in
    panic) printf '/// (Rust 1.60+)\n' ;;             # https://github.com/rust-lang/rust/pull/93658
    target_abi) printf '/// (Rust 1.78+)\n' ;;        # https://github.com/rust-lang/rust/pull/119590
    target_feature) printf '/// (Rust 1.27+)\n' ;;    # https://github.com/rust-lang/rust/pull/49664
    target_has_atomic) printf '/// (Rust 1.60+)\n' ;; # https://github.com/rust-lang/rust/pull/93824
    target_vendor) printf '/// (Rust 1.33+)\n' ;;     # https://github.com/rust-lang/rust/pull/57465
  esac
  if [[ -n "${STR:-}" ]]; then
    cat <<EOF
#[derive(Clone, PartialEq, Eq)]
pub struct ${name}(Box<str>);
EOF
  else
    local variant_str=("$@")
    variant_str+=($(grep -E --no-filename "^${cfg_name}=" tools/gen/cfg/1.*.txt | sed -E 's/^'"${cfg_name}"'="(.*)"$/\1/g'))
    # sort and dedup
    IFS=$'\n'
    variant_str=($(LC_ALL=C sort -u <<<"${variant_str[*]}"))
    IFS=$'\n\t'
    local variant_ident=()
    local has_num_start=''
    for v in "${variant_str[@]}"; do
      v="${v//-/_}"
      if [[ "${v}" =~ ^[0-9] ]]; then
        has_num_start=1
        v="_${v}"
      fi
      variant_ident+=("${v}")
    done
    if [[ -z "${has_num_start}" ]]; then
      if [[ -z "${EXHAUSTIVE:-}" ]]; then
        local non_exhaustive='#[non_exhaustive]'
      else
        local non_exhaustive='#[allow(clippy::exhaustive_enums)]'
      fi
      cat <<EOF
///
/// All non-empty values used in builtin targets in ${start} (where \`rustc --print cfg\` was added)
/// to ${stable} are available as variants. To construct other values, use
/// \`${name}::from\`/\`.into()\`; to reference them, use \`.as_str()\` or comparison to \`&str\`.
#[derive(Clone${EXHAUSTIVE:+", PartialEq, Eq"})]
${non_exhaustive}
pub enum ${name} {
EOF
    else
      local repr_name="${name}Repr"
      cat <<EOF
///
/// All values used in builtin targets in ${start} (where \`rustc --print cfg\` was added)
/// to ${stable} are constructable without allocation. To construct values, use
/// \`${name}::from\`/\`.into()\`; to reference them, use \`.as_str()\` or comparison to \`&str\`.
#[derive(Clone${EXHAUSTIVE:+", PartialEq, Eq"})]
pub struct ${name}(${repr_name});
#[derive(Clone${EXHAUSTIVE:+", PartialEq, Eq"})]
enum ${repr_name} {
EOF
    fi
    for v in "${variant_ident[@]}"; do
      [[ -n "${v}" ]] || continue
      printf '    %s,\n' "${v}"
    done
    if [[ -z "${EXHAUSTIVE:-}" ]]; then
      if [[ -z "${has_num_start}" ]]; then
        printf '    #[doc(hidden)]\n'
        printf '    #[deprecated(note = "do not use this variant directly; use as_str or compare with a string instead")]\n'
      fi
      printf '    __Other(Box<str>),\n'
    fi
    printf '}\n'
  fi
  cat <<EOF
impl ${name} {
    #[must_use]
    pub const fn as_str(&self) -> &str {
EOF
  if [[ -n "${STR:-}" ]]; then
    printf '        &self.0\n'
  else
    if [[ -z "${has_num_start}" ]]; then
      printf '        match self {\n'
    else
      printf '        match &self.0 {\n'
    fi
    for i in "${!variant_str[@]}"; do
      v="${variant_str[${i}]}"
      [[ -n "${v}" ]] || continue
      vi="${variant_ident[${i}]}"
      if [[ -z "${has_num_start}" ]]; then
        printf '            Self::%s => "%s",\n' "${vi}" "${v}"
      else
        printf '            %s::%s => "%s",\n' "${repr_name}" "${vi}" "${v}"
      fi
    done
    if [[ -z "${EXHAUSTIVE:-}" ]]; then
      if [[ -z "${has_num_start}" ]]; then
        printf '            #[allow(deprecated)]\n'
        printf '            Self::__Other(s) => s,\n'
      else
        printf '            %s::__Other(s) => s,\n' "${repr_name}"
      fi
    fi
    printf '        }\n'
  fi
  cat <<EOF
    }
}
EOF
  if [[ -z "${EXHAUSTIVE:-}" ]] && [[ -z "${STR:-}" ]]; then
    cat <<EOF
impl PartialEq for ${name} {
    fn eq(&self, other: &Self) -> bool {
EOF
    if [[ -z "${has_num_start}" ]]; then
      printf '        match (self, other) {\n'
    else
      printf '        match (&self.0, &other.0) {\n'
    fi
    cat <<EOF
            // Even after new variants are added, to ensure comparisons in old
            // code using __Other still work, compare strings if either is __Other.
EOF
    if [[ -z "${has_num_start}" ]]; then
      printf '            #[allow(deprecated)]\n'
      printf '            (Self::__Other(_), _) | (_, Self::__Other(_)) => self.as_str() == other.as_str(),\n'
    else
      printf '            (%s::__Other(_), _) | (_, %s::__Other(_)) => self.as_str() == other.as_str(),\n' "${repr_name}" "${repr_name}"
    fi
    cat <<EOF
            (this, other) => mem::discriminant(this) == mem::discriminant(other),
        }
    }
}
impl Eq for ${name} {}
EOF
  fi
  for str in 'str' 'String' 'Box<str>'; do
    deref='*'
    [[ "${str}" == 'str' ]] || deref+='*'
    cat <<EOF
impl PartialEq<${str}> for ${name} {
    fn eq(&self, other: &${str}) -> bool {
        *self.as_str() == ${deref}other
    }
}
impl PartialEq<&${str}> for ${name} {
    fn eq(&self, &other: &&${str}) -> bool {
        *self.as_str() == ${deref}other
    }
}
impl PartialEq<${name}> for ${str} {
    fn eq(&self, other: &${name}) -> bool {
        ${deref}self == *other.as_str()
    }
}
impl PartialEq<${name}> for &${str} {
    fn eq(&self, other: &${name}) -> bool {
        ${deref}*self == *other.as_str()
    }
}
EOF
  done
  if [[ -n "${NUM:-}" ]] && [[ -n "${has_num_start}" ]]; then
    # i32 for default numeric fallback
    for num in u16 i32 u32 u64; do
      cat <<EOF
impl PartialEq<${num}> for ${name} {
    fn eq(&self, other: &${num}) -> bool {
        match &self.0 {
EOF
      for i in "${!variant_str[@]}"; do
        v="${variant_str[${i}]}"
        [[ -n "${v}" ]] || continue
        vi="${variant_ident[${i}]}"
        if [[ "${v}" =~ ^[0-9]+$ ]]; then
          printf '            %s::%s => %s == *other,\n' "${repr_name}" "${vi}" "${v}"
        else
          printf '            %s::%s => false,\n' "${repr_name}" "${vi}"
        fi
      done
      if [[ -z "${EXHAUSTIVE:-}" ]]; then
        printf '            %s::__Other(this) => this.parse().ok() == Some(*other),\n' "${repr_name}"
      fi
      cat <<EOF
        }
    }
}
impl PartialEq<${num}> for &${name} {
    fn eq(&self, other: &${num}) -> bool {
        *self == other
    }
}
impl PartialEq<${name}> for ${num} {
    fn eq(&self, other: &${name}) -> bool {
        other == self
    }
}
impl PartialEq<&${name}> for ${num} {
    fn eq(&self, &other: &&${name}) -> bool {
        other == self
    }
}
EOF
    done
  fi
  if [[ -z "${EXHAUSTIVE:-}" ]] && [[ -z "${NUM:-}" ]]; then
    cat <<EOF
impl From<&str> for ${name} {
    fn from(s: &str) -> Self {
        match Self::from_str(s) {
            Ok(s) => s,
            Err(e) => match e {},
        }
    }
}
EOF
  fi
  cat <<EOF
impl FromStr for ${name} {
    type Err = ${error};
    fn from_str(s: &str) -> Result<Self, Self::Err> {
EOF
  if [[ -n "${STR:-}" ]]; then
    printf '        Ok(Self(s.into()))\n'
  else
    printf '        match s {\n'
    for i in "${!variant_str[@]}"; do
      v="${variant_str[${i}]}"
      [[ -n "${v}" ]] || continue
      vi="${variant_ident[${i}]}"
      if [[ -z "${has_num_start}" ]]; then
        printf '            "%s" => Ok(Self::%s),\n' "${v}" "${vi}"
      else
        printf '            "%s" => Ok(Self(%s::%s)),\n' "${v}" "${repr_name}" "${vi}"
      fi
    done
    if [[ -z "${EXHAUSTIVE:-}" ]]; then
      if [[ -z "${has_num_start}" ]]; then
        printf '            #[allow(deprecated)]\n'
        printf '            s => Ok(Self::__Other(s.into())),\n'
      else
        # TODO: check the value if [[ -n "${NUM:-}" ]]
        printf '            s => Ok(Self(%s::__Other(s.into()))),\n' "${repr_name}"
      fi
    else
      local msg="must be one of "
      for i in "${!variant_str[@]}"; do
        v="${variant_str[${i}]}"
        [[ -n "${v}" ]] || continue
        msg+="${v}, "
      done
      msg+="but found \`{other}\`"
      printf '            other => bail!("%s"),\n' "${msg}"
    fi
    printf '        }\n'
  fi
  cat <<EOF
    }
}
impl Cfg for ${name} {
EOF
  if [[ -z ${MULTIPLE:-} ]]; then
    if [[ -z ${ALWAYS_AVAILABLE:-} ]]; then
      cat <<EOF
    type Output = Option<Self>;
    const KEY: &'static str = "${cfg_name}";
    type Error = ${error};
    const MAX: usize = 1;
    fn from_values<I: ExactSizeIterator<Item = V>, V: AsRef<str>>(
        mut values: I,
    ) -> Result<Self::Output, Self::Error> {
        Ok(Some(values.next().unwrap().as_ref().parse()?))
    }
    fn default_output() -> Option<Self::Output> {
        Some(None)
    }
EOF
    else
      cat <<EOF
    type Output = Self;
    const KEY: &'static str = "${cfg_name}";
    type Error = ${error};
    const MAX: usize = 1;
    fn from_values<I: ExactSizeIterator<Item = V>, V: AsRef<str>>(
        mut values: I,
    ) -> Result<Self::Output, Self::Error> {
        values.next().unwrap().as_ref().parse()
    }
    fn default_output() -> Option<Self::Output> {
        None
    }
EOF
    fi
  else
    cat <<EOF
    type Output = Vec<Self>;
    const KEY: &'static str = "${cfg_name}";
    type Error = ${error};
    const MAX: usize = usize::MAX;
    fn from_values<I: ExactSizeIterator<Item = V>, V: AsRef<str>>(
        values: I,
    ) -> Result<Self::Output, Self::Error> {
        let mut res = Vec::with_capacity(values.len());
        for value in values {
            res.push(value.as_ref().parse()?);
        }
        Ok(res)
    }
    fn default_output() -> Option<Self::Output> {
        Some(vec![])
    }
EOF
  fi
  cat <<EOF
}
impl fmt::Debug for ${name} {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}
impl fmt::Display for ${name} {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
EOF
}
unset ALWAYS_AVAILABLE
unset EXHAUSTIVE
unset MULTIPLE
unset STR
unset NUM

# target_vendor is always shows in --print cfg at this time, but
# only accessible in code since 1.33 (https://github.com/rust-lang/rust/pull/57465),
# and maybe not so in the future (https://github.com/rust-lang/rust/issues/100343).
cat >|"${file}" <<EOF
// SPDX-License-Identifier: Apache-2.0 OR MIT
// This file is @generated by ${0##*/}.
// It is not intended for manual editing.

//! Trait and types for [\`Config::cfg\`](crate::Config::cfg).

#![cfg_attr(rustfmt, rustfmt::skip)]
#![allow(non_camel_case_types)]

use alloc::{boxed::Box, string::String, vec, vec::Vec};
use core::{fmt, mem, str::FromStr};

pub trait Cfg {
    /// The type returned from [\`Config::cfg\`](crate::Config::cfg) when no error occurred.
    type Output: Sized;
    /// The name of this cfg.
    const KEY: &'static str;
    /// (For use inside [\`Config::cfg\`](crate::Config::cfg).)
    type Error: Sized + std::error::Error + Send + Sync + 'static;
    /// (For use inside [\`Config::cfg\`](crate::Config::cfg).)
    ///
    /// The maximum number of this cfg.
    ///
    /// In all implementation provided by this crate, if \`Self::Output\` is a
    /// collection type such as \`Vec\` it is \`usize::MAX\`, otherwise \`1\`.
    const MAX: usize;
    /// (For use inside [\`Config::cfg\`](crate::Config::cfg).)
    ///
    /// The length of \`values\` is \`1..=Self::MAX\`.
    fn from_values<I: ExactSizeIterator<Item = V>, V: AsRef<str>>(
        values: I,
    ) -> Result<Self::Output, Self::Error>;
    /// (For use inside [\`Config::cfg\`](crate::Config::cfg).)
    ///
    /// The value returned if cfg is not found. If \`None\`, an error is returned in such a case.
    fn default_output() -> Option<Self::Output>;
}

$(enum target_abi)

$(ALWAYS_AVAILABLE=1 enum target_arch)

$(ALWAYS_AVAILABLE=1 EXHAUSTIVE=1 enum target_endian)

$(enum target_env)

$(MULTIPLE=1 enum target_family)

$(MULTIPLE=1 NUM=1 enum target_has_atomic)

$(ALWAYS_AVAILABLE=1 enum target_os)

$(ALWAYS_AVAILABLE=1 NUM=1 enum target_pointer_width)

$(enum target_vendor)
EOF

# TODO: currently our cfg getting method doesn't respect -C panic/-C target-feature/-C target-cpu
# $(enum panic)
# $(MULTIPLE=1 STR=1 enum target_feature)
