#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

SCRIPT_NAME="$(basename "$0")"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPENDENCIES=("rustup" "cargo")
APP_BIN_NAME="gestalt"
LAUNCHER_STATE_ROOT="${GESTALT_LAUNCHER_STATE_ROOT:-${PROJECT_ROOT}/.launcher-state}"
ISOLATE_STATE="${GESTALT_LAUNCHER_ISOLATE_STATE:-1}"
SMOKE_SECONDS="${GESTALT_LAUNCHER_SMOKE_SECONDS:-5}"
MANAGED_STATE_DIR=""

usage() {
  cat <<USAGE
Gestalt developer launcher.

Usage:
  ./${SCRIPT_NAME} --help
  ./${SCRIPT_NAME} --install
  ./${SCRIPT_NAME} --build
  ./${SCRIPT_NAME} --build-release
  ./${SCRIPT_NAME} --test
  ./${SCRIPT_NAME} --perf [-- <profile args...>]
  ./${SCRIPT_NAME} --release-smoke [-- <app args...>]
  ./${SCRIPT_NAME} --run [-- <app args...>]
  ./${SCRIPT_NAME} --run-release [-- <app args...>]

Required flags:
  --help           Show usage, flags, examples, and exit codes
  --install        Install missing launcher dependencies
  --build          Build debug binary
  --build-release  Build optimized release binary
  --test           Run the canonical test suite
  --perf           Run the performance gate with isolated state
  --release-smoke  Build and smoke-test the release app with isolated state
  --run            Start the app with cargo run
  --run-release    Start the built release binary

Examples:
  ./${SCRIPT_NAME} --install
  ./${SCRIPT_NAME} --build
  ./${SCRIPT_NAME} --build-release
  ./${SCRIPT_NAME} --test
  ./${SCRIPT_NAME} --perf
  ./${SCRIPT_NAME} --perf -- --json
  ./${SCRIPT_NAME} --release-smoke
  ./${SCRIPT_NAME} --run
  ./${SCRIPT_NAME} --run -- --example-flag value
  ./${SCRIPT_NAME} --run-release
  ./${SCRIPT_NAME} --run-release -- --example-flag value

Managed state:
  GESTALT_LAUNCHER_ISOLATE_STATE=1   Use repo-local isolated state dirs (default)
  GESTALT_LAUNCHER_ISOLATE_STATE=0   Use the host's normal app state locations
  GESTALT_LAUNCHER_STATE_ROOT=PATH   Override the repo-local launcher state root
  GESTALT_LAUNCHER_SMOKE_SECONDS=N   Override release smoke duration (default: 5)

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

is_windows_shell() {
  case "$(uname -s 2>/dev/null || true)" in
    CYGWIN*|MINGW*|MSYS*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

release_bin_path() {
  if is_windows_shell; then
    printf '%s\n' "${PROJECT_ROOT}/target/release/${APP_BIN_NAME}.exe"
    return 0
  fi

  printf '%s\n' "${PROJECT_ROOT}/target/release/${APP_BIN_NAME}"
}

managed_state_dir() {
  local mode="$1"
  printf '%s\n' "${LAUNCHER_STATE_ROOT}/${mode}"
}

make_temp_state_dir() {
  local mode="$1"
  mkdir -p "$LAUNCHER_STATE_ROOT"
  mktemp -d "${LAUNCHER_STATE_ROOT}/${mode}.XXXXXX"
}

setup_managed_state_env() {
  local state_dir="$1"
  local workspace_dir="${state_dir}/workspace"
  local emily_dir="${state_dir}/emily"
  local xdg_state_dir="${state_dir}/xdg-state"
  local xdg_data_dir="${state_dir}/xdg-data"

  mkdir -p "$workspace_dir" "$emily_dir" "$xdg_state_dir" "$xdg_data_dir"

  export GESTALT_WORKSPACE_PATH="${workspace_dir}/workspace.v1.json"
  export GESTALT_EMILY_DB_PATH="$emily_dir"
  export XDG_STATE_HOME="$xdg_state_dir"
  export XDG_DATA_HOME="$xdg_data_dir"

  log "[state] isolated state dir: $state_dir"
}

configure_managed_state() {
  local mode="$1"
  local lifespan="$2"
  local state_dir=""

  MANAGED_STATE_DIR=""

  if [[ "$ISOLATE_STATE" != "1" ]]; then
    log "[state] host state enabled (GESTALT_LAUNCHER_ISOLATE_STATE=$ISOLATE_STATE)"
    return 0
  fi

  if [[ "$lifespan" == "temp" ]]; then
    state_dir="$(make_temp_state_dir "$mode")"
  else
    state_dir="$(managed_state_dir "$mode")"
  fi

  setup_managed_state_env "$state_dir"
  MANAGED_STATE_DIR="$state_dir"
}

cleanup_state_dir() {
  local state_dir="$1"

  if [[ -n "$state_dir" && -d "$state_dir" ]]; then
    rm -rf "$state_dir"
  fi
}

run_with_optional_temp_state() {
  local mode="$1"
  shift
  local status=0

  configure_managed_state "$mode" "temp"
  "$@" || status=$?
  cleanup_state_dir "$MANAGED_STATE_DIR"
  return "$status"
}

validate_smoke_seconds() {
  [[ "$SMOKE_SECONDS" =~ ^[1-9][0-9]*$ ]] \
    || die "GESTALT_LAUNCHER_SMOKE_SECONDS must be a positive integer"
}

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
  configure_managed_state "dev" "persistent"
  exec cargo run --bin "$APP_BIN_NAME" -- "${run_args[@]}"
}

run_release_app() {
  local run_args=("$@")
  local release_bin=""
  cd "$PROJECT_ROOT"

  ensure_runtime_dependencies
  release_bin="$(release_bin_path)"

  if [[ ! -x "$release_bin" ]]; then
    log "missing release binary: $release_bin"
    log "run ./${SCRIPT_NAME} --build-release first"
    exit 4
  fi

  configure_managed_state "release" "persistent"
  exec "$release_bin" "${run_args[@]}"
}

run_tests() {
  cd "$PROJECT_ROOT"
  ensure_runtime_dependencies
  log "[test] cargo test -q"
  run_with_optional_temp_state "test" cargo test -q
}

run_perf() {
  local perf_args=("$@")
  cd "$PROJECT_ROOT"
  ensure_runtime_dependencies
  log "[perf] scripts/perf-gate.sh"
  run_with_optional_temp_state "perf" ./scripts/perf-gate.sh "${perf_args[@]}"
}

run_release_smoke() {
  local run_args=("$@")
  local release_bin=""
  local state_dir=""
  local pid=0

  cd "$PROJECT_ROOT"
  ensure_runtime_dependencies
  validate_smoke_seconds
  build_app "release"

  release_bin="$(release_bin_path)"
  if [[ ! -x "$release_bin" ]]; then
    log "missing release binary after build: $release_bin"
    exit 4
  fi

  configure_managed_state "release-smoke" "temp"
  state_dir="$MANAGED_STATE_DIR"
  log "[smoke] starting release binary for ${SMOKE_SECONDS}s"
  "$release_bin" "${run_args[@]}" &
  pid=$!

  for ((i = 0; i < SMOKE_SECONDS * 10; i++)); do
    if ! kill -0 "$pid" 2>/dev/null; then
      wait "$pid" || true
      cleanup_state_dir "$state_dir"
      die "release smoke failed: app exited before ${SMOKE_SECONDS}s window completed"
    fi
    sleep 0.1
  done

  log "[smoke] stopping release binary"
  kill -INT "$pid" 2>/dev/null || true

  for ((i = 0; i < 30; i++)); do
    if ! kill -0 "$pid" 2>/dev/null; then
      break
    fi
    sleep 0.1
  done

  if kill -0 "$pid" 2>/dev/null; then
    kill -TERM "$pid" 2>/dev/null || true
  fi

  wait "$pid" || true
  cleanup_state_dir "$state_dir"
  log "[done] release smoke passed"
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
      --test)
        [[ -z "$action" ]] || die_usage "only one action flag is allowed"
        action="test"
        shift
        ;;
      --perf)
        [[ -z "$action" ]] || die_usage "only one action flag is allowed"
        action="perf"
        shift
        ;;
      --release-smoke)
        [[ -z "$action" ]] || die_usage "only one action flag is allowed"
        action="release-smoke"
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
        [[ "$action" == "run" || "$action" == "run-release" || "$action" == "perf" || "$action" == "release-smoke" ]] \
          || die_usage "-- is only valid with --run, --run-release, --perf, or --release-smoke"
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
    test)
      ((${#run_args[@]} == 0)) || die_usage "--test does not accept app args"
      run_tests
      ;;
    perf)
      run_perf "${run_args[@]}"
      ;;
    release-smoke)
      run_release_smoke "${run_args[@]}"
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
