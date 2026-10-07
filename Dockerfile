FROM rust:bookworm AS build
WORKDIR /app
RUN rustup target add wasm32-unknown-unknown && cargo install wasm-bindgen-cli --version 0.2.104 --locked
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY web ./web
RUN cargo build --locked -p belote-client --target wasm32-unknown-unknown --release && \
    wasm-bindgen target/wasm32-unknown-unknown/release/belote_client.wasm --target web --out-dir web/pkg --no-typescript && \
    cargo build --locked -p belote-server --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /app/target/release/belote-server /app/belote-server
COPY --from=build /app/web /app/web
ENV BELOTE_BIND=0.0.0.0:3000 BELOTE_WEB_DIR=/app/web
USER 65532:65532
EXPOSE 3000
CMD ["/app/belote-server"]
