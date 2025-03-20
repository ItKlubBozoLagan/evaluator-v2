# Dockerfile that builds an x86_64 binary on any platform

FROM rust:bullseye

WORKDIR /app

CMD ["cargo", "build", "--target", "x86_64-unknown-linux-gnu"]
