#!/usr/bin/env zsh
autoload colors; colors
zmodload zsh/zutil
zparseopts -D -F -K -- \
    {p,-probe}:=arg_probe \
    {l,-log}:=arg_log ||
    return 1



function info {
    echo "${fg[green]}INFO: ${1}${reset_color}"
}

function error {
    echo ""
    echo "${fg[red]}Error: ${1}${reset_color}"
    exit 1
}

chip="stm32g0b1ketx"
build_target="${@[1]}"
project_root="${0:h1:a}"
binary_path="${build_target:a}"

cd ${project_root}

log_level=debug
if [[ -n "${arg_log}" ]]; then
    log_level="${arg_log}"
elif [[ ${build_target} == "release" ]]; then
     log_level="info"
fi

build_mode=("debug" "release")
if [[ -z ${build_target} ]]; then
    error "provide file or build mode(debug, release)"
elif (($build_mode[(I)$build_target])); then
    info "Run cargo build - mode: ${build_target} Log level: ${log_level}"
    export DEFMT_LOG="${log_level}"
    cargo build --${build_target}
    (( $? )) && {error "Failed cargo build with ${build_target} mode"}
    binary_path="${project_root}/target/thumbv6m-none-eabi/${build_target}/corne-eec-stm32g0"
elif [ ! -f "${binary_path}" ]; then
    error "${build_target} not exists."
fi

# pyocd write firmware more quickly than probe-rs.
pyocd_args=("load" "--target" "${chip}" "--format" "elf")
# use prove_run because probe_rs doesn't provide option to skipping flash.
probe_run_args=("--no-flash" "--chip" "${chip}")

[[ -n ${arg_probe} ]] && {
    pyocd_args+=("${arg_probe[@]}")
    probe_run_args+=("${arg_probe[@]}")
}

info "Load ${binary_path}"
pyocd "${pyocd_args[@]}" "${binary_path}"
(( $? )) && {error "Failed to flash with PyOcd"}

info "Run ${binary_path}"
probe-run "${probe_run_args[@]}" "${binary_path}"
