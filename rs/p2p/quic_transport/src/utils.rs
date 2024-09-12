//! Quic Transport utilities.
//!
//! Contains the actual wire format used for messages.
//! Request encoding Request<Bytes>:
//!     - Split into header and body.
//!     - Header contains a HeaderMap and the URI
//!     - Body is just the byte vector.
//!     - Both the header and body are encoded with bincode
//!     - At this point both header and body are just a vector of bytes.
//!       The two bytes vector both get length limited encoded and sent.
//!     - Reading a request involves doing two reads from the wire for the
//!       encoded header and body and reconstructing it into a typed request.
//! Response encoding Response<Bytes>:
//!     - Same as request expect that the header contains a HeaderMap and a Statuscode.
use axum::{
    body::{Body, HttpBody},
    extract::State,
    http::{Request, Response, StatusCode, Uri},
    middleware::Next,
};
use bincode::Options;
use bytes::Bytes;
use quinn::{ReadError, ReadToEndError, RecvStream, SendStream};
use serde::{Deserialize, Serialize};

use crate::{metrics::QuicTransportMetrics, SendError};

#[derive(Debug)]
pub(crate) enum RecvError {
    RecvRequestFailed { reason: String },
    SendResponseFailed { reason: String },
}

impl std::fmt::Display for RecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RecvRequestFailed { reason: e } => {
                write!(f, "Receiving a request failed: {}", e)
            }
            Self::SendResponseFailed { reason } => {
                write!(f, "Sending a response failed: {}", reason)
            }
        }
    }
}

/// On purpose the value is big, otherwise there is risk of not processing important consensus messages.
/// E.g. summary blocks generated by the consensus protocol for 40 node subnet can be bigger than 5MB.
const MAX_MESSAGE_SIZE_BYTES: usize = 128 * 1024 * 1024;

fn bincode_config() -> impl Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(MAX_MESSAGE_SIZE_BYTES as u64)
}

pub(crate) async fn read_request(mut recv_stream: RecvStream) -> Result<Request<Body>, RecvError> {
    let raw_msg = recv_stream
        .read_to_end(MAX_MESSAGE_SIZE_BYTES)
        .await
        .map_err(|_| RecvError::RecvRequestFailed {
            reason: format!(
                "Recv stream for request contains more than {} bytes",
                MAX_MESSAGE_SIZE_BYTES
            ),
        })?;
    let msg: WireRequest =
        bincode_config()
            .deserialize(&raw_msg)
            .map_err(|err| RecvError::RecvRequestFailed {
                reason: format!("Deserializing request failed: {}", err),
            })?;

    let mut request = Request::new(Body::from(Bytes::copy_from_slice(msg.body)));
    let _ = std::mem::replace(request.uri_mut(), msg.uri);
    Ok(request)
}

pub(crate) async fn read_response(
    mut recv_stream: RecvStream,
) -> Result<Response<Bytes>, SendError> {
    let raw_msg = recv_stream
        .read_to_end(MAX_MESSAGE_SIZE_BYTES)
        .await
        .map_err(|err| match err {
            ReadToEndError::Read(ReadError::ConnectionLost(conn_err)) => conn_err.into(),
            ReadToEndError::TooLong => SendError::Internal(format!(
                "Recv stream for response contains more than {} bytes",
                MAX_MESSAGE_SIZE_BYTES
            )),
            _ => SendError::Internal(err.to_string()),
        })?;
    let msg: WireResponse = bincode_config()
        .deserialize(&raw_msg)
        .map_err(|err| SendError::Internal(format!("Deserializing response failed: {}", err)))?;

    let mut response = Response::new(Bytes::copy_from_slice(msg.body));
    let _ = std::mem::replace(response.status_mut(), msg.status);
    Ok(response)
}

pub(crate) async fn write_request(
    send_stream: &mut SendStream,
    request: Request<Bytes>,
) -> Result<(), SendError> {
    let (parts, body) = request.into_parts();

    let msg = WireRequest {
        uri: parts.uri,
        body: &body,
    };

    let res = bincode_config()
        .serialize(&msg)
        .map_err(|err| SendError::Internal(err.to_string()))?;
    Ok(send_stream.write_all(&res).await?)
}

pub(crate) async fn write_response(
    send_stream: &mut SendStream,
    response: Response<Body>,
) -> Result<(), RecvError> {
    let (parts, body) = response.into_parts();
    // Check for axum error in body
    // TODO: Think about this. What is the error that can happen here?
    let b = axum::body::to_bytes(body, MAX_MESSAGE_SIZE_BYTES)
        .await
        .map_err(|err| RecvError::SendResponseFailed {
            reason: err.to_string(),
        })?;
    let msg = WireResponse {
        status: parts.status,
        body: &b,
    };

    let res = bincode_config()
        .serialize(&msg)
        .map_err(|err| RecvError::SendResponseFailed {
            reason: err.to_string(),
        })?;
    send_stream
        .write_all(&res)
        .await
        .map_err(|err| RecvError::SendResponseFailed {
            reason: err.to_string(),
        })
}

#[derive(Serialize, Deserialize)]
struct WireResponse<'a> {
    #[serde(with = "http_serde::status_code")]
    status: StatusCode,
    #[serde(with = "serde_bytes")]
    body: &'a [u8],
}

#[derive(Serialize, Deserialize)]
struct WireRequest<'a> {
    #[serde(with = "http_serde::uri")]
    uri: Uri,
    #[serde(with = "serde_bytes")]
    body: &'a [u8],
}

/// Axum middleware to collect metrics
pub(crate) async fn collect_metrics(
    State(state): State<QuicTransportMetrics>,
    request: Request<Body>,
    next: Next,
) -> axum::response::Response {
    state
        .request_handle_bytes_received_total
        .with_label_values(&[request.uri().path()])
        .inc_by(request.body().size_hint().lower());
    let _timer = state
        .request_handle_duration_seconds
        .with_label_values(&[request.uri().path()])
        .start_timer();
    let out_counter = state
        .request_handle_bytes_sent_total
        .with_label_values(&[request.uri().path()]);
    let response = next.run(request).await;
    out_counter.inc_by(response.body().size_hint().lower());
    response
}