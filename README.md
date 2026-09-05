# kontestis-evaluator-rs

Redis-backed evaluator worker using IOI `isolate`.

## Target configuration

```text
REDIS_URL=rediss://user:password@redis.redis.svc.cluster.local:6379/0
REDIS_CA_FILE=/etc/redis-ca/ca.crt
REDIS_REQUIRE_TLS=true
REDIS_QUEUE_KEY=kontestis:production:evaluator:requests
EVALUATOR_MAX_EVALUATIONS=2
RUN_WITH_CGROUPS=true
RUN_WITH_QUOTAS=false
COMPILE_CACHE_ENABLED=false
```

Optional Redis settings are `REDIS_CONNECTION_TIMEOUT_MS` (default `5000`),
`REDIS_COMMAND_TIMEOUT_MS` (default `5000`), and `REDIS_PUBLISH_ATTEMPTS`
(default `5`). `EVALUATOR_HEALTH_BIND` defaults to `0.0.0.0:8080` and serves
`/live`, `/ready`, and `/metrics`.

## Build

The binary is built outside Docker so CI can reuse its Rust `sccache`:

```sh
cargo build --release --locked
docker build --build-arg SOURCE_REVISION="$(git rev-parse HEAD)" -t kontestis-evaluator-v2 .
```

The container must run privileged and requires writable
`/var/local/lib/isolate` and `/run/isolate` mounts.
