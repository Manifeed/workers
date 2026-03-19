use std::fs;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use directories::ProjectDirs;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{Result, WorkerError};
use crate::types::WorkerType;

const IDENTITY_FILE_NAME: &str = "identity.json";

#[derive(Clone, Debug)]
pub struct LocalIdentity {
    path: PathBuf,
    stored: StoredIdentity,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredIdentity {
    device_id: String,
    signing_key_b64: String,
    public_key_b64: String,
    enrolled: bool,
}

impl LocalIdentity {
    pub fn load_or_create(identity_dir: &Path) -> Result<Self> {
        fs::create_dir_all(identity_dir)?;
        let path = identity_dir.join(IDENTITY_FILE_NAME);
        if path.exists() {
            let stored = serde_json::from_slice::<StoredIdentity>(&fs::read(&path)?)?;
            return Ok(Self { path, stored });
        }

        let stored = generate_stored_identity(Uuid::new_v4().to_string());
        fs::write(&path, serde_json::to_vec_pretty(&stored)?)?;
        Ok(Self { path, stored })
    }

    pub fn device_id(&self) -> &str {
        &self.stored.device_id
    }

    pub fn public_key_b64(&self) -> &str {
        &self.stored.public_key_b64
    }

    pub fn enrolled(&self) -> bool {
        self.stored.enrolled
    }

    pub fn mark_enrolled(&mut self) -> Result<()> {
        self.stored.enrolled = true;
        self.persist()
    }

    pub fn reset_for_reenrollment(&mut self) -> Result<()> {
        self.stored = generate_stored_identity(self.stored.device_id.clone());
        self.persist()
    }

    pub fn fingerprint(&self) -> Result<String> {
        let public_key_bytes = BASE64
            .decode(self.stored.public_key_b64.as_bytes())
            .map_err(|error| WorkerError::Crypto(error.to_string()))?;
        Ok(format!("{:x}", Sha256::digest(public_key_bytes)))
    }

    pub fn sign_challenge(&self, challenge_id: &str, challenge: &str) -> Result<String> {
        let signing_key = self.signing_key()?;
        let message = format!("{challenge_id}:{challenge}");
        Ok(BASE64.encode(signing_key.sign(message.as_bytes()).to_bytes()))
    }

    pub fn summary(&self, worker_type: WorkerType) -> Result<String> {
        Ok(format!(
            "{}:{}:{}",
            worker_type.as_str(),
            self.device_id(),
            self.fingerprint()?
        ))
    }

    fn signing_key(&self) -> Result<SigningKey> {
        let signing_key_bytes = BASE64
            .decode(self.stored.signing_key_b64.as_bytes())
            .map_err(|error| WorkerError::Crypto(error.to_string()))?;
        let signing_key_bytes: [u8; 32] = signing_key_bytes
            .try_into()
            .map_err(|_| WorkerError::Crypto("invalid signing key length".to_string()))?;
        Ok(SigningKey::from_bytes(&signing_key_bytes))
    }

    #[allow(dead_code)]
    fn verifying_key(&self) -> Result<VerifyingKey> {
        let public_key_bytes = BASE64
            .decode(self.stored.public_key_b64.as_bytes())
            .map_err(|error| WorkerError::Crypto(error.to_string()))?;
        let public_key_bytes: [u8; 32] = public_key_bytes
            .try_into()
            .map_err(|_| WorkerError::Crypto("invalid public key length".to_string()))?;
        VerifyingKey::from_bytes(&public_key_bytes)
            .map_err(|error| WorkerError::Crypto(error.to_string()))
    }

    fn persist(&self) -> Result<()> {
        fs::write(&self.path, serde_json::to_vec_pretty(&self.stored)?)?;
        Ok(())
    }
}

fn generate_stored_identity(device_id: String) -> StoredIdentity {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    StoredIdentity {
        device_id,
        signing_key_b64: BASE64.encode(signing_key.to_bytes()),
        public_key_b64: BASE64.encode(verifying_key.to_bytes()),
        enrolled: false,
    }
}

pub fn default_identity_dir(worker_type: WorkerType) -> Result<PathBuf> {
    let project_dirs =
        ProjectDirs::from("io", "manifeed", worker_type.as_str()).ok_or_else(|| {
            WorkerError::Config("unable to resolve worker identity directory".to_string())
        })?;
    Ok(project_dirs.config_dir().to_path_buf())
}
