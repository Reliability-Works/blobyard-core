# syntax=docker/dockerfile:1.7
FROM rust:1.96.0-bookworm@sha256:5e2214abe154fe26e39f64488952e5c991eeed1d6d6da7cc8381ae83927f0cfc AS build

WORKDIR /source
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY openapi ./openapi
RUN cargo build --locked --release --package blobyard-server --bin blobyard-server

FROM gcr.io/distroless/cc-debian13:nonroot@sha256:aded2458d026e046cb68199db0e5793e1028ffa143f7258f3c4278253e20add7
ARG BLOBYARD_REVISION=unknown
ARG BLOBYARD_VERSION=unknown
LABEL org.opencontainers.image.licenses="Apache-2.0" \
  org.opencontainers.image.revision="$BLOBYARD_REVISION" \
  org.opencontainers.image.source="https://github.com/Reliability-Works/blobyard-core" \
  org.opencontainers.image.title="Blob Yard standalone server" \
  org.opencontainers.image.version="$BLOBYARD_VERSION"

COPY --from=build --chown=nonroot:nonroot /source/target/release/blobyard-server /usr/local/bin/blobyard-server
COPY --chown=nonroot:nonroot deploy/docker/data /var/lib/blobyard

EXPOSE 8787
VOLUME ["/var/lib/blobyard"]
HEALTHCHECK --interval=10s --timeout=5s --start-period=10s --retries=6 \
  CMD ["/usr/local/bin/blobyard-server", "healthcheck"]
ENTRYPOINT ["/usr/local/bin/blobyard-server"]
CMD ["serve", "--listen", "0.0.0.0:8787", "--data-dir", "/var/lib/blobyard/data", "--public-url", "http://localhost:8787", "--web-yard-origin", "http://localhost:8787"]
