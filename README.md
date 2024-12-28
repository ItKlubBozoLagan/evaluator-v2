# kontestis-evaluator-rs

### Docker
```bash
docker run --cgroupns=host --privileged --add-host=host.docker.internal:host-gateway --env RUN_WITH_CGROUPS=true --env RUN_WITH_QUOTAS=false --env REDIS_URL=redis://host.docker.internal:6379 --env FORCE_DEBUG_LOGS=true --rm -it kontestis-evaluator-v2:latest
```
