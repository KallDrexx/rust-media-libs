#!/bin/bash

if [ "$#" -ne 2 ]; then
    echo "Expected 2 arguments: ./run-perf.sh <FlameGraph Path> <SVG file name>"
fi

RUSTFLAGS=-g

cargo build --release
perf record -ag ../../target/release/video-relay
perf script | $1/stackcollapse-perf.pl | $1/flamegraph.pl > $2