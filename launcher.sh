#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

SCRIPT_NAME="$(basename "$0")"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPENDENCIES=("rustup" "cargo")
APP_BIN_NAME="gestalt"
RELEASE_BIN_PATH="${PROJECT_ROOT}/target/release/${APP_BIN_NAME}"

usage() {
  cat <<USAGE
Gestalt developer launcher.

Usage:
  ./${SCRIPT_NAME} --help
  ./${SCRIPT_NAME} --install
  ./${SCRIPT_NAME} --build
  ./${SCRIPT_NAME} --build-release
  ./${SCRIPT_NAME} --run [-- <app args...>]
  ./${SCRIPT_NAME} --run-release [-- <app args...>]

Required flags:
  --help           Show usage, flags, examples, and exit codes
  --install        Install missing launcher dependencies
  --build          Build debug binary
  --build-release  Build optimized release binary
  --run            Start the app with cargo run
  --run-release    Start the built release binary

Examples:
  ./${SCRIPT_NAME} --install
  ./${SCRIPT_NAME} --build
  ./${SCRIPT_NAME} --build-release
  ./${SCRIPT_NAME} --run
  ./${SCRIPT_NAME} --run -- --example-flag value
  ./${SCRIPT_NAME} --run-release
  ./${SCRIPT_NAME} --run-release -- --example-flag value

Exit codes:
  0 success
  1 operation failed
  2 usage error
  3 missing dependency for runtime
  4 missing release artifact
  130 interrupted (SIGINT)
USAGE
}

log() {
  printf '[launcher] %s\n' "$*"
}

die() {
  log "error: $*"
  exit 1
}

die_usage() {
  log "usage error: $*"
  usage
  exit 2
}

on_sigint() {
  log "interrupted"
  exit 130
}

trap on_sigint INT

check_rustup() {
  command -v rustup >/dev/null 2>&1
}

install_rustup() {
  die "install rustup first: https://rustup.rs"
}

check_cargo() {
  command -v cargo >/dev/null 2>&1
}

install_cargo() {
  if ! check_rustup; then
    die "cannot install cargo: rustup is not installed"
  fi

  rustup toolchain install stable --profile minimal
}

check_dep() {
  "check_$1"
}

install_dep() {
  "install_$1"
}

install_dependencies() {
  local dep
  for dep in "${DEPENDENCIES[@]}"; do
    if check_dep "$dep"; then
      log "[ok] $dep already satisfied"
      continue
    fi

    log "[install] $dep missing; installing"
    if install_dep "$dep"; then
      if check_dep "$dep"; then
        log "[done] $dep installed"
      else
        log "[error] $dep install failed verification"
        exit 1
      fi
    else
      log "[error] $dep install failed"
      exit 1
    fi
  done
}

ensure_runtime_dependencies() {
  local dep
  for dep in "${DEPENDENCIES[@]}"; do
    if ! check_dep "$dep"; then
      log "missing dependency: $dep"
      log "run ./${SCRIPT_NAME} --install first"
      exit 3
    fi
  done
}

build_app() {
  local mode="$1"
  cd "$PROJECT_ROOT"

  ensure_runtime_dependencies

  case "$mode" in
    debug)
      log "[build] compiling debug binary: $APP_BIN_NAME"
      cargo build --bin "$APP_BIN_NAME"
      ;;
    release)
      log "[build] compiling release binary: $APP_BIN_NAME"
      cargo build --release --bin "$APP_BIN_NAME"
      ;;
    *)
      die_usage "invalid build mode: $mode"
      ;;
  esac
}

run_app() {
  local run_args=("$@")
  cd "$PROJECT_ROOT"

  ensure_runtime_dependencies
  exec cargo run --bin "$APP_BIN_NAME" -- "${run_args[@]}"
}

run_release_app() {
  local run_args=("$@")
  cd "$PROJECT_ROOT"

  ensure_runtime_dependencies

  if [[ ! -x "$RELEASE_BIN_PATH" ]]; then
    log "missing release binary: $RELEASE_BIN_PATH"
    log "run ./${SCRIPT_NAME} --build-release first"
    exit 4
  fi

  exec "$RELEASE_BIN_PATH" "${run_args[@]}"
}

main() {
  local action=""
  local run_args=()

  while (($#)); do
    case "$1" in
      --help)
        [[ -z "$action" ]] || die_usage "only one action flag is allowed"
        action="help"
        shift
        ;;
      --install)
        [[ -z "$action" ]] || die_usage "only one action flag is allowed"
        action="install"
        shift
        ;;
      --build)
        [[ -z "$action" ]] || die_usage "only one action flag is allowed"
        action="build"
        shift
        ;;
      --build-release)
        [[ -z "$action" ]] || die_usage "only one action flag is allowed"
        action="build-release"
        shift
        ;;
      --run)
        [[ -z "$action" ]] || die_usage "only one action flag is allowed"
        action="run"
        shift
        ;;
      --run-release)
        [[ -z "$action" ]] || die_usage "only one action flag is allowed"
        action="run-release"
        shift
        ;;
      --)
        [[ "$action" == "run" || "$action" == "run-release" ]] \
          || die_usage "-- is only valid with --run or --run-release"
        shift
        run_args=("$@")
        break
        ;;
      *)
        die_usage "unknown argument: $1"
        ;;
    esac
  done

  [[ -n "$action" ]] || die_usage "one action flag is required"

  case "$action" in
    help)
      usage
      ;;
    install)
      ((${#run_args[@]} == 0)) || die_usage "--install does not accept app args"
      install_dependencies
      ;;
    build)
      ((${#run_args[@]} == 0)) || die_usage "--build does not accept app args"
      build_app "debug"
      ;;
    build-release)
      ((${#run_args[@]} == 0)) || die_usage "--build-release does not accept app args"
      build_app "release"
      ;;
    run)
      run_app "${run_args[@]}"
      ;;
    run-release)
      run_release_app "${run_args[@]}"
      ;;
    *)
      die_usage "invalid action: $action"
      ;;
  esac
}

main "$@"
