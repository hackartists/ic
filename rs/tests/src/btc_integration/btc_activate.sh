#!/bin/sh
cp /config/bitcoin.conf /tmp/bitcoin.conf
# read the digest produced by the load so we can create a container with
# this image
image_sha=$(docker load -i /config/image.tar| grep -oE 'sha256:\S+')
docker run --name=bitcoind-node -d \
    --net=host \
    -v /tmp:/bitcoin/.bitcoin \
    "$image_sha" -rpcbind=[::]:8332 -rpcallowip=::/0
