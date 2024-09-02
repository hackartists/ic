#!/bin/sh

# read the digest produced by the load so we can create a container with
# this image
image_sha=$(docker load -i /config/image.tar | grep -oE 'sha256:\S+')
docker run -d --name jaeger \
    -e COLLECTOR_OTLP_ENABLED=true \
    -e SPAN_STORAGE_TYPE=badger \
    -e BADGER_DIRECTORY_VALUE=/badger/data \
    -e BADGER_DIRECTORY_KEY=/badger/key \
    -p 4317:4317 \
    -p 16686:16686 \
    "$image_sha"
