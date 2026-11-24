# syntax=docker/dockerfile:1

FROM --platform=$BUILDPLATFORM rust:1.96-bookworm AS build

ARG TARGETARCH
RUN test "$TARGETARCH" = amd64 || { echo "this image is only built for amd64, not $TARGETARCH" >&2; exit 1; }
RUN rustup target add x86_64-unknown-linux-gnu \
    && if [ "$(uname -m)" != x86_64 ]; then \
        apt-get update \
        && apt-get install -y --no-install-recommends gcc-x86-64-linux-gnu libc6-dev-amd64-cross \
        && rm -rf /var/lib/apt/lists/*; \
    fi

ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
    CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p asteroids-server --target x86_64-unknown-linux-gnu \
    && cp target/x86_64-unknown-linux-gnu/release/asteroids-server /asteroids-server

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /asteroids-server /usr/local/bin/asteroids-server
EXPOSE 8443/udp
ENTRYPOINT ["/usr/local/bin/asteroids-server"]
