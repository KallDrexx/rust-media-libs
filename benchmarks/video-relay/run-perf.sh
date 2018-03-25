#!/bin/bash

if [ "$#" -lt 2 ]; then
    echo "Expected at least 2 arguments: ./run-perf.sh <FlameGraph Path> <SVG file name> [iteration count]"
    exit 1
fi

RUSTFLAGS=-g

cargo build --release
perf record -ag ../../target/release/video-relay $3
perf script | $1/stackcollapse-perf.pl | $1/flamegraph.pl > $2