# kontestis-evaluator-rs

### Redis

Redis TLS with a private CA can be configured with:

```sh
REDIS_URL=rediss://user:password@redis.example:6379/0
REDIS_CA_FILE=/etc/redis-ca/ca.crt
REDIS_REQUIRE_TLS=true
REDIS_QUEUE_KEY=evaluator_msg_queue
```

Optional settings are `REDIS_CONNECTION_TIMEOUT_MS` (default `5000`),
`REDIS_COMMAND_TIMEOUT_MS` (default `5000`), and `REDIS_PUBLISH_ATTEMPTS`
(default `5`).

### Docker
A built release binary is needed for the docker build to work
```sh
cargo build --release
docker build -t kontestis-evaluator-v2 .
```

```sh
docker run --privileged --env RUN_WITH_CGROUPS=true --env RUN_WITH_QUOTAS=false --add-host=host.docker.internal:host-gateway --env REDIS_URL=redis://host.docker.internal:6379 kontestis-evaluator-v2:latest
```
