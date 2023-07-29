# syntax=docker/dockerfile:1.2

## Builder

FROM rust:1.71.0-slim as builder
COPY . /src
WORKDIR /src/nport-server
RUN apt-get update && apt-get -y install libssl-dev pkg-config
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
ENV HTTP_PORT=3000
ENV HTTPS_PORT=3443
ENV PUBLIC_HTTPS_PORT=443
ENV CERTS_DIR=/certs
ENV ACME_STAGING=false
ENV DOMAIN=nport.io
ENV ACME_EMAIL=alberto@garciahierro.com
ENV CLOUDFLARE_ZONE_ID=2c87e47cfbda08dda91be7c6b31cef68
ENV ACME_DOMAIN=nport.io

EXPOSE 3000
EXPOSE 3443
EXPOSE 32768-60999/tcp
EXPOSE 32768-60999/udp

CMD ["/usr/local/bin/nport-server"]
