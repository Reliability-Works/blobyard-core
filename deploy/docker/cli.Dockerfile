# syntax=docker/dockerfile:1.7
FROM gcr.io/distroless/cc-debian13:debug-nonroot@sha256:c0fac88234e23b954d75f48b5d3a1a85c5e712bc57d312aca27f304b57d615c3 AS verify

ARG TARGETARCH
SHELL ["/busybox/sh", "-c"]
WORKDIR /work
COPY packaging/docker/SHA256SUMS ./SHA256SUMS
COPY packaging/docker/bin ./bin
RUN grep "  bin/${TARGETARCH}/blobyard$" SHA256SUMS | sha256sum -c -

FROM gcr.io/distroless/cc-debian13:nonroot@sha256:aded2458d026e046cb68199db0e5793e1028ffa143f7258f3c4278253e20add7
ARG TARGETARCH
ARG BLOBYARD_REVISION=unknown
ARG BLOBYARD_VERSION=unknown
LABEL org.opencontainers.image.licenses="Apache-2.0" \
  org.opencontainers.image.revision="$BLOBYARD_REVISION" \
  org.opencontainers.image.source="https://github.com/Reliability-Works/blobyard-core" \
  org.opencontainers.image.title="Blobyard CLI" \
  org.opencontainers.image.version="$BLOBYARD_VERSION"
COPY --from=verify --chown=nonroot:nonroot /work/bin/${TARGETARCH}/blobyard /usr/local/bin/blobyard

ENTRYPOINT ["/usr/local/bin/blobyard"]
