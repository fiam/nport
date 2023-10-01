#!/bin/bash

set -x
set -e

DEST=$1

docker buildx build \
    --platform linux/amd64 \
    --progress=plain \
    -f nport-server/deploy/Dockerfile . \
    -t nport-server:latest 

docker save nport-server:latest | bzip2 | ssh ${DEST} docker load
ssh ${DEST} systemctl restart nport-server
