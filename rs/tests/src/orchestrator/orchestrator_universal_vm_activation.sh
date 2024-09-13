#!/bin/sh
##########################################################################################
############ Configures Universal VM to run static file serving on HTTP ##################
##########################################################################################

mkdir web
cd web
cp /config/registry.tar .
chmod -R 755 ./

docker pull halverneus/static-file-server@sha256:c387c31ffb55ac5b6b4654bc9924f73eb8fb5214ebb8552a7eeffc8849f0e7dd
docker tag halverneus/static-file-server@sha256:c387c31ffb55ac5b6b4654bc9924f73eb8fb5214ebb8552a7eeffc8849f0e7dd static-file-server
docker run -d \
    -v "$(pwd)":/web \
    -p 80:8080 \
    static-file-server
