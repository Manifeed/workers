use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::error::{Result, WorkerError};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerConnectionCheck {
    pub ok: bool,
    pub worker_type: Option<String>,
    pub worker_name: Option<String>,
    pub checked_at: DateTime<Utc>,
    pub status_code: Option<u16>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct WorkerPingRead {
    ok: bool,
    worker_type: String,
    worker_name: String,
}

pub fn check_worker_connection(api_url: &str, api_key: &str) -> Result<WorkerConnectionCheck> {
    let response = Client::new()
        .get(format!(
            "{}/workers/api/ping",
            api_url.trim_end_matches('/')
        ))
        .bearer_auth(api_key)
        .send()?;
    let status = response.status();

    if status != StatusCode::OK {
        let body = response.text().unwrap_or_else(|_| String::new());
        return Ok(WorkerConnectionCheck {
            ok: false,
            worker_type: None,
            worker_name: None,
            checked_at: Utc::now(),
            status_code: Some(status.as_u16()),
            error: Some(body),
        });
    }

    let payload = response
        .json::<WorkerPingRead>()
        .map_err(WorkerError::Http)?;
    Ok(WorkerConnectionCheck {
        ok: payload.ok,
        worker_type: Some(payload.worker_type),
        worker_name: Some(payload.worker_name),
        checked_at: Utc::now(),
        status_code: Some(StatusCode::OK.as_u16()),
        error: None,
    })
}
