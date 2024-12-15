FROM rust:alpine AS builder

WORKDIR /app/src
RUN USER=root

RUN apk add --no-cache \
    musl-dev  \
    pkgconfig \
    openssl \
    openssl-dev \
    libc-dev \
    build-base

ENV RUSTFLAGS="-C target-feature=-crt-static -C link-arg=-lgcc_eh"

COPY ./ ./
RUN cargo build --release

FROM alpine:latest
WORKDIR /app
RUN apk update && apk add --no-cache  \
    openssl \
    ca-certificates

EXPOSE 3333

COPY --from=builder /app/src/target/release/budgetbeast /app/budgetbeast

ENTRYPOINT ["/app/budgetbeast"]