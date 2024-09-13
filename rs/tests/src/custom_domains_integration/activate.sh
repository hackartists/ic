#!/usr/bin/env bash

# Load docker images for various services. Services are started by the test driver.

set -euo pipefail

function load() {
    NAME=$1

    # Load image
    docker load -i "/config/${NAME}.tar"

    # Rename image
    docker tag \
        bazel/image:image "${NAME}"

    # Remove temporary image
    docker rmi bazel/image:image
}

load coredns
load pebble
load python3
load openssl
