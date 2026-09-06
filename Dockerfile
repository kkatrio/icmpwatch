FROM rust:alpine3.22

WORKDIR /usr/src/icmpwatch
COPY Cargo.toml .
COPY src/ src/

RUN cargo install --path .

CMD ["icmpwatch"]
