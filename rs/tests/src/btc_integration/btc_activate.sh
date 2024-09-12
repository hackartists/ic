#!/bin/sh

set -euo pipefail

cp /config/bitcoin.conf /tmp/bitcoin.conf
echo "loading docker image for bitcoin"
ls /config/image.tar
docker load -i /config/image.tar
echo "loaded"
docker run --name=bitcoind-node -d \
    --net=host \
    -v /tmp:/bitcoin/.bitcoin \
    bazel/image:image -rpcbind=[::]:8332 -rpcallowip=::/0
