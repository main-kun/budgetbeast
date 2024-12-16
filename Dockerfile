FROM rust:alpine AS builder

WORKDIR /app/src
RUN USER=root

RUN apk add --no-cache \
    musl-dev  \
    pkgconfig \
    openssl \
    openssl-dev \
    libc-dev \
    build-base \
    openssl-libs-static

RUN rustup target add x86_64-unknown-linux-musl

ENV RUSTFLAGS="-C link-arg=-static" \
    CARGO_BUILD_TARGET="x86_64-unknown-linux-musl"

COPY ./ ./
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM alpine:latest
WORKDIR /app
RUN apk update && apk add --no-cache  \
    openssl \
    ca-certificates

EXPOSE 3333

COPY --from=builder /app/src/target/x86_64-unknown-linux-musl/release/budgetbeast /app/budgetbeast

ENTRYPOINT ["/app/budgetbeast"]