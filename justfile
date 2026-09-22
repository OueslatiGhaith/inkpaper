set shell := ["bash", "-cu"]

firmware_package := "inkpaper-x4-pro"
simulator_package := "inkpaper-simulator"
x4_pro_target := "xtensa-esp32s3-none-elf"

firmware_elf := "target/" + x4_pro_target + "/firmware/" + firmware_package 

default:
    @just --list

test:
    cargo nextest run

sim:
    cargo run -p {{simulator_package}}

x4-run:
    cargo +inkpaper-esp run -p {{firmware_package}} --target {{x4_pro_target}} --profile firmware

x4-perf features="performance,ui-metrics":
    cargo +inkpaper-esp run -p {{firmware_package}} \
        --target {{x4_pro_target}} --profile profiling --features {{features}}

x4-trace:
    just x4-perf "performance,ui-metrics,trace"

x4-perf-log output="target/perf/latest.log":
    mkdir -p target/perf/history
    if [[ -f "{{output}}" ]]; then \
        stamp="$(date '+%Y%m%d-%H%M%S')"; \
        archive="target/perf/history/perf-${stamp}.log"; \
        suffix=1; \
        while [[ -e "${archive}" ]]; do \
            archive="target/perf/history/perf-${stamp}-${suffix}.log"; \
            suffix=$((suffix + 1)); \
        done; \
        cp "{{output}}" "${archive}"; \
        cp "{{output}}" target/perf/before.log; \
        echo "previous run: target/perf/before.log"; \
        echo "archived:     ${archive}"; \
    fi
    just x4-perf 2>&1 | tee "{{output}}"

x4-perf-report log="target/perf/latest.log":
    cargo xtask perf summary "{{log}}"

x4-perf-compare before="target/perf/before.log" after="target/perf/latest.log":
    cargo xtask perf compare "{{before}}" "{{after}}"

x4-trace-log output="target/perf/latest-trace.log":
    mkdir -p target/perf/trace-history
    if [[ -f "{{output}}" ]]; then \
        stamp="$(date '+%Y%m%d-%H%M%S')"; \
        archive="target/perf/trace-history/trace-${stamp}.log"; \
        suffix=1; \
        while [[ -e "${archive}" ]]; do \
            archive="target/perf/trace-history/trace-${stamp}-${suffix}.log"; \
            suffix=$((suffix + 1)); \
        done; \
        mv "{{output}}" "${archive}"; \
        echo "archived trace: ${archive}"; \
    fi
    just x4-trace 2>&1 | tee "{{output}}"

x4-trace-report log="target/perf/latest-trace.log":
    cargo xtask perf summary "{{log}}"
    cargo xtask trace-perfetto "{{log}}"