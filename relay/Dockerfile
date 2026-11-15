# relay/Dockerfile — 🔒 fijar las imágenes base por digest (@sha256:…) en el PR real
FROM rust:1.100-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p procastinacioff-relay

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/procastinacioff-relay /relay
USER nonroot:nonroot
ENV PCO_BIND=0.0.0.0:8080 PCO_DB=/data/relay.db PCO_TIME_KEY=/data/time.key
VOLUME ["/data"]
EXPOSE 8080
ENTRYPOINT ["/relay"]
