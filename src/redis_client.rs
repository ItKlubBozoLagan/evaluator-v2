use crate::environment::Environment;
use redis::{Client, TlsCertificates};

pub fn build_client() -> anyhow::Result<Client> {
    let environment = Environment::get();

    if environment.redis_url.contains("#insecure") {
        anyhow::bail!("insecure Redis TLS verification is not supported");
    }

    if environment.redis_url.starts_with("rediss://") {
        let ca_file = environment
            .redis_ca_file
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("REDIS_CA_FILE is required for rediss://"))?;
        let root_cert = std::fs::read(ca_file).map_err(|err| {
            anyhow::anyhow!("failed to read Redis CA file {}: {err}", ca_file.display())
        })?;
        if root_cert.is_empty() {
            anyhow::bail!("Redis CA file {} is empty", ca_file.display());
        }

        return Client::build_with_tls(
            environment.redis_url.as_str(),
            TlsCertificates {
                client_tls: None,
                root_cert: Some(root_cert),
            },
        )
        .map_err(|err| anyhow::anyhow!("failed to configure Redis TLS: {err}"));
    }

    if environment.redis_require_tls {
        anyhow::bail!("REDIS_URL must use rediss:// when REDIS_REQUIRE_TLS=true");
    }

    Client::open(environment.redis_url.as_str())
        .map_err(|err| anyhow::anyhow!("invalid REDIS_URL: {err}"))
}
