# kontestis-evaluator-rs

Redis-backed evaluator worker using IOI `isolate`. The worker has no public API;
its HTTP listener only exposes internal health and metrics endpoints.

## Configuration

Target deployments should set at least:

```text
REDIS_URL=rediss://user:password@redis.redis.svc.cluster.local:6379/0
REDIS_CA_FILE=/etc/redis-ca/ca.crt
REDIS_REQUIRE_TLS=true
REDIS_QUEUE_KEY=kontestis:production:evaluator:requests
REDIS_RESPONSE_KEY_PREFIX=kontestis:production:evaluator:results:
REDIS_DEAD_LETTER_KEY=kontestis:production:evaluator:dead-letter
EVALUATOR_MAX_EVALUATIONS=2
EVALUATOR_MEMORY_BUDGET_MIB=6144
EVALUATOR_SYSTEM_MEMORY_RESERVE_MIB=1024
RUN_WITH_CGROUPS=true
RUN_WITH_QUOTAS=false
COMPILE_CACHE_ENABLED=false
```

The worker also accepts these bounded-operation settings:

| Variable | Default |
| --- | ---: |
| `REDIS_CONNECTION_TIMEOUT_MS` | `5000` |
| `REDIS_COMMAND_TIMEOUT_MS` | `5000` |
| `REDIS_PUBLISH_ATTEMPTS` | `5` |
| `EVALUATOR_MAX_REQUEST_BYTES` | `67108864` |
| `EVALUATOR_MAX_SOURCE_BYTES` | `1048576` |
| `EVALUATOR_MAX_CHECKER_BYTES` | `1048576` |
| `EVALUATOR_MAX_TESTCASES` | `256` |
| `EVALUATOR_MAX_TESTCASE_BYTES` | `67108864` |
| `EVALUATOR_MAX_OUTPUT_BYTES` | `1048576` |
| `EVALUATOR_JOB_TIMEOUT_SECONDS` | `300` |
| `EVALUATOR_HEALTH_BIND` | `0.0.0.0:8080` |

`REDIS_RESPONSE_KEY_PREFIX` should include its trailing separator. Requests with
a response key outside that prefix are dead-lettered. Redis URL credentials are
never logged.

The internal listener provides `GET /live`, `GET /ready`, and `GET /metrics`.
Readiness is false while Redis is unavailable or the worker is draining.

## Build

```sh
cargo test --locked
docker build --build-arg SOURCE_REVISION="$(git rev-parse HEAD)" -t kontestis-evaluator-v2 .
```

The container must run privileged for `isolate`. The deployment must mount
writable `/tmp`, `/var/local/lib/isolate`, and `/run/isolate` volumes. Its
termination grace period must exceed `EVALUATOR_JOB_TIMEOUT_SECONDS` plus the
configured Redis publication retry window.

Compile caching remains available for existing development use but must stay
disabled in target deployments until it is separately bounded and tested.
