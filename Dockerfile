FROM rust:alpine3.24 AS builder
WORKDIR /usr/src/app

COPY Cargo.toml Cargo.toml
COPY src/ src/

RUN cargo install --path .

FROM alpine:3.24
ENV TZ=Europe/Athens
RUN apk add --no-cache tzdata && \
    ln -snf /usr/share/zoneinfo/$TZ /etc/localtime && \
    echo $TZ > /etc/timezone

COPY --from=builder /usr/local/cargo/bin/icmpwatch /usr/local/bin/icmpwatch
CMD ["icmpwatch"]
