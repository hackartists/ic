#!/bin/sh
##########################################################################################
############ Configures Universal VM to run static file serving on HTTP ##################
##########################################################################################

mkdir web
cd web
cp /config/registry.tar .
chmod -R 755 ./

# read the digest produced by the load so we can create a container with
# this image
image_sha=$(docker load -i /config/static-file-server.tar | grep -oE 'sha256:\S+')
docker run -d \
    -v "$(pwd)":/web \
    -p 80:8080 \
    "$image_sha"
