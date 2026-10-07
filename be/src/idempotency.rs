//! `Idempotency-Key` sulle POST: la stessa chiave con lo stesso corpo restituisce la risposta già data.
//!
//! Vengono memorizzate solo le risposte 2xx: dopo un errore la chiave è di nuovo libera, così il
//! client può correggere lo stato (es. confermare un override FEFO) e riprovare.

use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::error::ApiError;

const MAX_BODY: usize = 4 * 1024 * 1024;

pub async fn layer(State(pool): State<PgPool>, req: Request, next: Next) -> Response {
    if req.method() != Method::POST {
        return next.run(req).await;
    }
    let Some(key) = req
        .headers()
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
    else {
        return next.run(req).await;
    };
    if key.is_empty() || key.len() > 200 {
        return ApiError::validation("Idempotency-Key non valida (1-200 caratteri)").into_response();
    }
    handle(&pool, key, req, next)
        .await
        .unwrap_or_else(IntoResponse::into_response)
}

async fn handle(pool: &PgPool, key: String, req: Request, next: Next) -> Result<Response, ApiError> {
    let (parts, body) = req.into_parts();
    let bytes = to_bytes(body, MAX_BODY)
        .await
        .map_err(|_| ApiError::validation("Corpo della richiesta troppo grande"))?;
    let path = parts.uri.path().to_string();
    let mut hasher = Sha256::new();
    hasher.update(parts.method.as_str());
    hasher.update(b"\n");
    hasher.update(&path);
    hasher.update(b"\n");
    hasher.update(&bytes);
    let hash = hex::encode(hasher.finalize());

    let inserted = sqlx::query(
        "INSERT INTO idempotency_keys (key, method, path, request_hash) VALUES ($1, $2, $3, $4)
         ON CONFLICT (key) DO NOTHING",
    )
    .bind(&key)
    .bind(parts.method.as_str())
    .bind(&path)
    .bind(&hash)
    .execute(pool)
    .await?
    .rows_affected()
        == 1;

    if !inserted {
        let row: (String, Option<i32>, Option<Vec<u8>>) =
            sqlx::query_as("SELECT request_hash, status_code, response_body FROM idempotency_keys WHERE key = $1")
                .bind(&key)
                .fetch_one(pool)
                .await?;
        return match row {
            (stored, _, _) if stored != hash => Err(ApiError::unprocessable(
                "IDEMPOTENCY_KEY_REUSED",
                "Idempotency-Key già usata per una richiesta diversa",
            )
            .with(json!({ "key": key }))),
            (_, Some(status), Some(body)) => {
                let mut res = Response::new(Body::from(body));
                *res.status_mut() = StatusCode::from_u16(status as u16).unwrap_or(StatusCode::OK);
                res.headers_mut()
                    .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
                res.headers_mut()
                    .insert("idempotent-replayed", HeaderValue::from_static("true"));
                Ok(res)
            }
            _ => Err(ApiError::conflict(
                "IDEMPOTENCY_IN_PROGRESS",
                "Una richiesta con la stessa Idempotency-Key è ancora in esecuzione",
            )
            .with(json!({ "key": key }))),
        };
    }

    let res = next.run(Request::from_parts(parts, Body::from(bytes))).await;
    let status = res.status();
    if !status.is_success() {
        let _ = sqlx::query("DELETE FROM idempotency_keys WHERE key = $1")
            .bind(&key)
            .execute(pool)
            .await;
        return Ok(res);
    }
    let (parts, body) = res.into_parts();
    let bytes = to_bytes(body, usize::MAX).await.map_err(|_| ApiError::internal())?;
    sqlx::query("UPDATE idempotency_keys SET status_code = $2, response_body = $3 WHERE key = $1")
        .bind(&key)
        .bind(status.as_u16() as i32)
        .bind(bytes.to_vec())
        .execute(pool)
        .await?;
    Ok(Response::from_parts(parts, Body::from(bytes)))
}
