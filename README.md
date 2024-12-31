# kontestis-evaluator-rs

### Docker
```bash
docker run --privileged --add-host=host.docker.internal:host-gateway --env RUN_WITH_CGROUPS=true --env RUN_WITH_QUOTAS=false --env REDIS_URL=redis://host.docker.internal:6379 kontestis-evaluator-v2:latest
```
