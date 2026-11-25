# Turtle as a container image: the web app and the server are built here, and
# the image that runs holds only the binary, which embeds the web app and the
# PDF fonts. Build it from the repository root with `docker build .`.

FROM docker.io/library/node:24-bookworm-slim AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
# The operation types are committed and checked by scripts/check.sh, so the
# bundle is built without generating them, which would need the server.
RUN npx vite build --logLevel warn

FROM docker.io/library/rust:1.100-slim-bookworm AS server
WORKDIR /src/server
COPY server/ ./
COPY --from=web /src/web/dist /src/web/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/server/target \
    cargo build --release --locked \
    && cp target/release/turtle /turtle
# The data directory, owned by the user Turtle runs as, so a fresh named
# volume takes that owner.
RUN mkdir /data

# Only the C runtime Rust binaries link against: no shell, no package
# manager, no build tools, and a non-root user.
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=server /turtle /usr/local/bin/turtle
COPY --from=server --chown=nonroot:nonroot /data /data
ENV TURTLE_DATA=/data \
    TURTLE_HOST=0.0.0.0 \
    TURTLE_PORT=4100
VOLUME /data
EXPOSE 4100
USER nonroot
ENTRYPOINT ["/usr/local/bin/turtle"]
