# syntax=docker/dockerfile:1.2

## Builder
ARG RUST_VERSION=1.71.0
ARG PROTOC_VERSION=24.3

FROM rust:$RUST_VERSION-slim as builder
ENV PROTOC_VERSION=24.3
COPY . /src
WORKDIR /src/nport-server
RUN apt-get update && apt-get -y install libssl-dev pkg-config curl unzip
RUN curl -LO https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-linux-x86_64.zip \
    && unzip protoc-${PROTOC_VERSION}-linux-x86_64.zip \
    && mv bin/protoc /usr/local/bin/protoc
RUN --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,sharing=private,target=/src/target \
    cargo build --target x86_64-unknown-linux-gnu --release
RUN --mount=type=cache,sharing=private,target=/src/target \
    cp /src/target/x86_64-unknown-linux-gnu/release/nport-server /nport-server

## Runner

# debian:bullseye-slim is the base for rust:1.71.0-slim
FROM debian:bullseye-slim AS runner
RUN apt-get update && apt-get -y install libssl1.1 ca-certificates
COPY --from=builder /nport-server /usr/local/bin
# RUN /sbin/sysctl -w net.ipv4.ip_local_port_range="32768	60999"

ENV RELEASE=true
ENV BIND_ADDRESS=0.0.0.0
ENV HTTP_PORT=80
ENV HTTPS_PORT=443
ENV CERTS_DIR=/certs
ENV ACME_STAGING=false
ENV DOMAIN=nport.io
ENV API_SUBDOMAIN=api
ENV TCP_SUBDOMAIN=tcp
ENV ACME_EMAIL=alberto@garciahierro.com
ENV ACME_PERSIST_DIR=/acme
ENV ACME_DOMAIN=nport.io
ENV CLOUDFLARE_ZONE_ID=2c87e47cfbda08dda91be7c6b31cef68

EXPOSE 3000
EXPOSE 3443
EXPOSE 32768-60999/tcp
EXPOSE 32768-60999/udp

CMD ["/usr/local/bin/nport-server"]
