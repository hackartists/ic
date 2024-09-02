#!/run/current-system/sw/bin/bash

function load() {
    NAME=$1

    # read the digest produced by the load so we can create a container with
    # this image
    image_sha=$(docker load -i "/config/${NAME}.tar" | grep -oE 'sha256:\S+')

    # Rename image
    docker tag \
        "$image_sha" "${NAME}"

    # Remove temporary image
    docker rmi "$image_sha"
}

load coredns
load pebble
load python3
load openssl
