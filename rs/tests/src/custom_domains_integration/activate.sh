#!/run/current-system/sw/bin/bash

function load() {
    NAME=$1

    echo loading "$NAME"

    # Load image
    docker load -i "/config/${NAME}.tar"
}

load coredns
load pebble
load python3
load openssl
