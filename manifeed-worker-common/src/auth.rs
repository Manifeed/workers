use std::path::PathBuf;

use chrono::{Duration, Utc};
use hostname::get;

use crate::api::ApiClient;
use crate::error::{Result, WorkerError};
use crate::identity::{default_identity_dir, LocalIdentity};
use crate::types::{
    WorkerAuthChallengeRead, WorkerAuthChallengeRequest, WorkerAuthVerifyRequest,
    WorkerEnrollRequest, WorkerSessionRead, WorkerType,
};

#[derive(Clone, Debug)]
pub struct WorkerAuthConfig {
    pub worker_type: WorkerType,
    pub identity_dir: Option<PathBuf>,
    pub enrollment_token: Option<String>,
    pub worker_version: String,
}

pub struct WorkerAuthenticator {
    config: WorkerAuthConfig,
    identity: LocalIdentity,
    session: Option<WorkerSessionRead>,
}

impl WorkerAuthenticator {
    pub fn new(config: WorkerAuthConfig) -> Result<Self> {
        let identity_dir = config
            .identity_dir
            .clone()
            .unwrap_or(default_identity_dir(config.worker_type)?);
        let identity = LocalIdentity::load_or_create(&identity_dir)?;
        Ok(Self {
            config,
            identity,
            session: None,
        })
    }

    pub fn identity_summary(&self) -> Result<String> {
        self.identity.summary(self.config.worker_type)
    }

    pub fn device_id(&self) -> &str {
        self.identity.device_id()
    }

    pub async fn ensure_session(&mut self, api_client: &ApiClient) -> Result<String> {
        if let Some(session) = &self.session {
            if session.expires_at > Utc::now() + Duration::seconds(60) {
                return Ok(session.access_token.clone());
            }
        }

        let session = if self.identity.enrolled() {
            match self.request_auth_challenge(api_client).await {
                Ok(challenge) => self.verify_challenge(api_client, challenge).await?,
                Err(error) if is_unknown_worker_identity_error(&error) => {
                    self.identity.reset_for_reenrollment()?;
                    self.enroll_with_current_identity(api_client).await?
                }
                Err(error) => return Err(error),
            }
        } else {
            self.enroll_with_current_identity(api_client).await?
        };

        let access_token = session.access_token.clone();
        self.session = Some(session);
        Ok(access_token)
    }

    async fn verify_challenge(
        &self,
        api_client: &ApiClient,
        challenge: WorkerAuthChallengeRead,
    ) -> Result<WorkerSessionRead> {
        api_client
            .post_json(
                "/workers/auth/verify",
                &WorkerAuthVerifyRequest {
                    worker_type: self.config.worker_type,
                    device_id: self.identity.device_id().to_string(),
                    challenge_id: challenge.challenge_id.clone(),
                    signature: self
                        .identity
                        .sign_challenge(&challenge.challenge_id, &challenge.challenge)?,
                },
                None,
            )
            .await
    }

    async fn request_auth_challenge(
        &self,
        api_client: &ApiClient,
    ) -> Result<WorkerAuthChallengeRead> {
        api_client
            .post_json::<_, WorkerAuthChallengeRead>(
                "/workers/auth/challenge",
                &WorkerAuthChallengeRequest {
                    worker_type: self.config.worker_type,
                    device_id: self.identity.device_id().to_string(),
                },
                None,
            )
            .await
    }

    async fn enroll_with_current_identity(
        &mut self,
        api_client: &ApiClient,
    ) -> Result<WorkerSessionRead> {
        let enrollment_token = self.config.enrollment_token.clone().ok_or_else(|| {
            WorkerError::Auth(
                "worker is not enrolled and no enrollment token is configured".to_string(),
            )
        })?;
        let challenge = api_client
            .post_json::<_, WorkerAuthChallengeRead>(
                "/workers/enroll",
                &WorkerEnrollRequest {
                    worker_type: self.config.worker_type,
                    device_id: self.identity.device_id().to_string(),
                    public_key: self.identity.public_key_b64().to_string(),
                    hostname: resolve_hostname(),
                    platform: Some(std::env::consts::OS.to_string()),
                    arch: Some(std::env::consts::ARCH.to_string()),
                    worker_version: Some(self.config.worker_version.clone()),
                    enrollment_token,
                },
                None,
            )
            .await?;
        let session = self.verify_challenge(api_client, challenge).await?;
        self.identity.mark_enrolled()?;
        Ok(session)
    }
}

fn resolve_hostname() -> Option<String> {
    get().ok().and_then(|hostname| hostname.into_string().ok())
}

fn is_unknown_worker_identity_error(error: &WorkerError) -> bool {
    matches!(
        error,
        WorkerError::Api { status, message }
            if *status == 404 && message == "Unknown worker identity"
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::tempdir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[tokio::test]
    async fn ensure_session_reenrolls_when_backend_forgets_identity() {
        let identity_dir = tempdir().expect("create temp identity dir");
        let identity_path = identity_dir.path().join("identity.json");

        let mut seeded_identity =
            LocalIdentity::load_or_create(identity_dir.path()).expect("seed identity");
        seeded_identity
            .mark_enrolled()
            .expect("mark identity enrolled");

        let original_identity =
            serde_json::from_slice::<serde_json::Value>(&fs::read(&identity_path).unwrap())
                .expect("read original identity");
        let original_device_id = original_identity["device_id"]
            .as_str()
            .expect("original device_id")
            .to_string();
        let original_public_key = original_identity["public_key_b64"]
            .as_str()
            .expect("original public key")
            .to_string();

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/workers/auth/challenge"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(json!({ "detail": "Unknown worker identity" })),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/workers/enroll"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "identity_id": 11,
                "challenge_id": "enroll_123",
                "challenge": "challenge-value"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/workers/auth/verify"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "worker-token",
                "expires_at": "2099-01-01T00:00:00Z",
                "worker_profile": {
                    "identity_id": 11,
                    "worker_type": "rss_scrapper",
                    "device_id": original_device_id,
                    "fingerprint": "fingerprint",
                    "display_name": null,
                    "hostname": null,
                    "platform": null,
                    "arch": null,
                    "worker_version": "0.1.0",
                    "enrollment_status": "enrolled",
                    "last_enrolled_at": null,
                    "last_auth_at": null
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let mut authenticator = WorkerAuthenticator::new(WorkerAuthConfig {
            worker_type: WorkerType::RssScrapper,
            identity_dir: Some(identity_dir.path().to_path_buf()),
            enrollment_token: Some("manifeed-rss-enroll".to_string()),
            worker_version: "0.1.0".to_string(),
        })
        .expect("create authenticator");
        let api_client = ApiClient::new(mock_server.uri()).expect("create api client");

        let token = authenticator
            .ensure_session(&api_client)
            .await
            .expect("refresh session after identity reset");

        assert_eq!(token, "worker-token");

        let refreshed_identity =
            serde_json::from_slice::<serde_json::Value>(&fs::read(&identity_path).unwrap())
                .expect("read refreshed identity");
        assert_eq!(
            refreshed_identity["device_id"].as_str(),
            Some(original_device_id.as_str())
        );
        assert_ne!(
            refreshed_identity["public_key_b64"].as_str(),
            Some(original_public_key.as_str())
        );
        assert_eq!(refreshed_identity["enrolled"].as_bool(), Some(true));
    }
}
