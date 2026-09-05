ARG RUST_IMAGE=rust:1.93-bookworm@sha256:7c4ae649a84014c467d79319bbf17ce2632ae8b8be123ac2fb2ea5be46823f31
ARG DEBIAN_IMAGE=debian:bookworm@sha256:6ebd97fa83deb272194a2cf015b3d26a4d538e9ad3a7a79d544c8af5b0a01443

FROM ${RUST_IMAGE} AS evaluator-build
RUN apt-get update \
 && apt-get install -y --no-install-recommends libseccomp-dev pkg-config \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM ${DEBIAN_IMAGE} AS isolate-build
ARG ISOLATE_REVISION=8f185bb37f3f23e29b33b0c7727c91c13429abe3
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates git gcc libc6-dev pkg-config make libcap-dev libsystemd-dev libseccomp-dev \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /opt
RUN git clone https://github.com/ioi/isolate \
 && git -C isolate checkout "${ISOLATE_REVISION}"
WORKDIR /opt/isolate
RUN make isolate isolate-cg-keeper default.cf

FROM ${DEBIAN_IMAGE}
ARG SOURCE_REVISION=unknown
LABEL org.opencontainers.image.source="https://github.com/ItKlubBozoLagan/evaluator-v2" \
      org.opencontainers.image.revision="${SOURCE_REVISION}"

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates gcc g++ golang libseccomp2 ocaml openjdk-17-jdk-headless python3 rustc \
 && rm -rf /var/lib/apt/lists/* \
 && ln -sf /usr/bin/gcc /usr/bin/cc \
 && ln -sf /usr/lib/jvm/java-17-openjdk-*/bin/javac /usr/bin/javac \
 && ln -sf /usr/lib/jvm/java-17-openjdk-*/bin/java /usr/bin/java

COPY --from=isolate-build /opt/isolate/isolate /usr/local/bin/isolate
COPY --from=isolate-build /opt/isolate/isolate-cg-keeper /usr/local/bin/isolate-cg-keeper
COPY --from=isolate-build /opt/isolate/default.cf /usr/local/etc/isolate

RUN useradd -M -U isolate \
 && echo "isolate:100000:65536" >> /etc/subuid \
 && echo "isolate:100000:65536" >> /etc/subgid

WORKDIR /app
COPY --from=evaluator-build /build/target/release/kontestis-evaluator-v2 /app/evaluator
COPY .docker /app/docker
RUN chmod +x /app/docker/*.sh

EXPOSE 8080
CMD ["/app/docker/entry.sh"]
