//! Estrattori e risposte HTTP comuni: errori di input come problem+json, operatore, ETag.

use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::error::ApiError;

/// Header `X-Operator`, obbligatorio sulle richieste di scrittura.
pub struct Operator(pub String);

impl<S: Send + Sync> FromRequestParts<S> for Operator {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let value = parts
            .headers
            .get("x-operator")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or_default();
        if value.is_empty() || value.len() > 100 {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "OPERATOR_MISSING",
                "L'header X-Operator è obbligatorio (max 100 caratteri)",
            ));
        }
        Ok(Operator(value.to_string()))
    }
}

/// Header `If-Match` opzionale, con la versione della risorsa.
pub struct IfMatch(pub Option<i32>);

impl<S: Send + Sync> FromRequestParts<S> for IfMatch {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let Some(raw) = parts.headers.get(header::IF_MATCH) else {
            return Ok(IfMatch(None));
        };
        let text = raw.to_str().unwrap_or_default().trim();
        let text = text.strip_prefix("W/").unwrap_or(text).trim_matches('"');
        text.parse::<i32>()
            .map(|v| IfMatch(Some(v)))
            .map_err(|_| ApiError::validation("If-Match non valido: usare l'ETag restituito dal GET"))
    }
}

impl IfMatch {
    pub fn check(&self, current: i32) -> Result<(), ApiError> {
        match self.0 {
            Some(expected) if expected != current => Err(ApiError::new(
                StatusCode::PRECONDITION_FAILED,
                "VERSION_CONFLICT",
                "La risorsa è stata modificata da un'altra richiesta: ricaricarla e riprovare",
            )
            .with(json!({ "expected": expected, "current": current }))),
            _ => Ok(()),
        }
    }
}

/// Corpo JSON obbligatorio, con errori di deserializzazione restituiti come `VALIDATION_ERROR`.
pub struct Body<T>(pub T);

/// Corpo JSON opzionale: un corpo vuoto produce `T::default()`.
pub struct OptBody<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for Body<T> {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = read_body(req, state).await?;
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Err(ApiError::validation("Il corpo della richiesta è obbligatorio"));
        }
        parse_json(&bytes).map(Body)
    }
}

impl<S: Send + Sync, T: DeserializeOwned + Default> FromRequest<S> for OptBody<T> {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = read_body(req, state).await?;
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok(OptBody(T::default()));
        }
        parse_json(&bytes).map(OptBody)
    }
}

async fn read_body<S: Send + Sync>(req: Request, state: &S) -> Result<Bytes, ApiError> {
    Bytes::from_request(req, state)
        .await
        .map_err(|e| ApiError::validation(format!("Corpo della richiesta non leggibile: {e}")))
}

fn parse_json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ApiError> {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    serde_path_to_error::deserialize(&mut de).map_err(|e| {
        let field = e.path().to_string();
        let message = e.inner().to_string();
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            format!("Campo `{field}`: {message}"),
        )
        .with(json!({ "errors": [{ "field": field, "message": message }] }))
    })
}

/// Query string con errori restituiti come `VALIDATION_ERROR`.
pub struct Query<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequestParts<S> for Query<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        axum::extract::Query::<T>::from_request_parts(parts, state)
            .await
            .map(|q| Query(q.0))
            .map_err(|e| ApiError::validation(format!("Parametri di query non validi: {}", e.body_text())))
    }
}

/// Parametri di path; un UUID non valido equivale a una risorsa inesistente.
pub struct Path<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned + Send> FromRequestParts<S> for Path<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        axum::extract::Path::<T>::from_request_parts(parts, state)
            .await
            .map(|p| Path(p.0))
            .map_err(|_| ApiError::new(StatusCode::NOT_FOUND, "NOT_FOUND", "Risorsa non trovata"))
    }
}

/// Risposta JSON. I decimali vengono normalizzati (`240.000` → `240`).
pub struct Json<T>(pub StatusCode, pub T, pub HeaderMap);

pub fn ok<T: Serialize>(body: T) -> Json<T> {
    Json(StatusCode::OK, body, HeaderMap::new())
}

pub fn created<T: Serialize>(body: T) -> Json<T> {
    Json(StatusCode::CREATED, body, HeaderMap::new())
}

impl<T> Json<T> {
    pub fn etag(mut self, version: i32) -> Self {
        if let Ok(v) = HeaderValue::from_str(&format!("\"{version}\"")) {
            self.2.insert(header::ETAG, v);
        }
        self
    }
}

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        match serde_json::to_value(&self.1) {
            Ok(mut value) => {
                normalize_numbers(&mut value);
                (self.0, self.2, axum::Json(value)).into_response()
            }
            Err(err) => {
                tracing::error!(error = %err, "serializzazione della risposta fallita");
                ApiError::internal().into_response()
            }
        }
    }
}

/// Rimuove gli zeri decimali superflui dai numeri (i NUMERIC di Postgres conservano la scala).
pub fn normalize_numbers(value: &mut Value) {
    match value {
        Value::Number(n) => {
            let text = n.to_string();
            if text.contains('.') && !text.contains(['e', 'E']) {
                let trimmed = text.trim_end_matches('0').trim_end_matches('.');
                if trimmed != text
                    && let Ok(v) = serde_json::from_str::<Value>(trimmed)
                {
                    *value = v;
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normalize_numbers),
        Value::Object(map) => map.values_mut().for_each(normalize_numbers),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_decimal_scale() {
        let mut v: Value = serde_json::from_str(r#"{"a": 240.000, "b": [1.500, 2], "c": 0.000}"#).unwrap();
        normalize_numbers(&mut v);
        assert_eq!(v.to_string(), r#"{"a":240,"b":[1.5,2],"c":0}"#);
    }
}
