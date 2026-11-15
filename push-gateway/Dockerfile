# push-gateway/Dockerfile — 🔒 fijar las imágenes base por digest (@sha256:…) en el PR real
FROM rust:1.100-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p procastinacioff-push-gateway

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/procastinacioff-push-gateway /gateway
USER nonroot:nonroot
ENV PG_BIND=0.0.0.0:8090
EXPOSE 8090
ENTRYPOINT ["/gateway"]
