FROM rust:alpine3.22

WORKDIR /usr/src/socketwatch
COPY Cargo.toml .
COPY src/ src/

RUN cargo install --path .

CMD ["socketwatch"]
