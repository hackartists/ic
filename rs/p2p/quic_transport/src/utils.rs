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
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap, HashMap},
};

use anyhow::Context;
use axum::{
    body::{Body, HttpBody},
    extract::State,
    http::{Request, Response, StatusCode, Uri},
    middleware::Next,
};
use bincode::Options;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use quinn::{Chunk, RecvStream, SendStream};
use reed_solomon_erasure::ReedSolomon;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use crate::metrics::QuicTransportMetrics;

/// On purpose the value is big, otherwise there is risk of not processing important consensus messages.
/// E.g. summary blocks generated by the consensus protocol for 40 node subnet can be bigger than 5MB.
const MAX_MESSAGE_SIZE_BYTES: usize = 128 * 1024 * 1024;

fn bincode_config() -> impl Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(MAX_MESSAGE_SIZE_BYTES as u64)
}

#[derive(Debug, Clone, Copy)]
struct EcHeader {
    len: u32,
    scheme: (u32, u32),
    padding: u32,
}

async fn assemble(mut recv_stream: RecvStream) -> Result<Bytes, anyhow::Error> {
    let chunk_size = 1280;
    let ec_header_size = 4 + 4 + 4 + 4;
    let tot_ec_size = chunk_size + ec_header_size;

    let mut header = None;

    let mut chunk_map: HashMap<usize, BinaryHeap<_>> = HashMap::new();
    let mut decoded_map: BTreeMap<usize, Bytes> = BTreeMap::new();
    while let Some(Chunk { offset, mut bytes }) = recv_stream
        .read_chunk(MAX_MESSAGE_SIZE_BYTES, false)
        .await?
    {
        println!("read chunk at {offset} size {}", bytes.len());
        let mut pos = offset as usize;
        while !bytes.is_empty() {
            let ec_idx = pos / tot_ec_size;
            // offset is at chunk boundary
            let c = if pos % tot_ec_size == 0 {
                if bytes.len() < tot_ec_size {
                    pos += bytes.len();
                    let b = bytes.split_to(bytes.len());
                    assert!(bytes.is_empty());
                    b
                } else {
                    pos += tot_ec_size;
                    assert!(pos % tot_ec_size == 0);
                    bytes.split_to(tot_ec_size)
                }
            } else {
                let dist_to_next_ec = tot_ec_size - (pos % tot_ec_size);
                if bytes.len() < dist_to_next_ec {
                    pos += bytes.len();
                    let b = bytes.split_to(bytes.len());
                    assert!(bytes.is_empty());
                    b
                } else {
                    pos += dist_to_next_ec;
                    assert!(pos % tot_ec_size == 0);
                    bytes.split_to(dist_to_next_ec)
                }
            };
            let e = chunk_map.entry(ec_idx).or_default();
            e.push((Reverse(pos), c));

            let sum = e.iter().map(|c| c.1.len()).sum::<usize>();
            if sum > tot_ec_size {
                println!("ahh {decoded_map:?} {sum}");
            }
            assert!(sum <= tot_ec_size);
            if sum == tot_ec_size {
                let mut bm = BytesMut::new();
                while let Some((Reverse(_), s)) = e.pop() {
                    bm.extend_from_slice(&s);
                }

                let mut ec_header = bm.split_off(bm.len() - ec_header_size);
                if header.is_none() {
                    header = Some(EcHeader {
                        len: ec_header.get_u32(),
                        scheme: (ec_header.get_u32(), ec_header.get_u32()),
                        padding: ec_header.get_u32(),
                    });
                    println!("{header:?} recvd");
                }
                decoded_map.insert(ec_idx, bm.freeze());
            }
        }
        println!("decopdec map {}", decoded_map.len());

        let h1 = header.clone();
        if h1.is_some_and(|h| h.scheme.0 <= decoded_map.len() as u32) {
            let header = h1.unwrap();
            println!("assembling {header:?}");
            let shards: (Vec<(usize, Bytes)>, Vec<(usize, Bytes)>) =
                decoded_map
                    .into_iter()
                    .fold((Vec::new(), Vec::new()), |mut acc, x| {
                        if x.0 < header.scheme.0 as usize {
                            acc.0.push(x);
                        } else {
                            acc.1.push((x.0 - header.scheme.1 as usize, x.1));
                        }
                        acc
                    });
            let mut v: Vec<_> = if shards.0.len() == header.scheme.0 as usize {
                shards.0.into_iter().map(|(i, b)| (i, b.to_vec())).collect()
            } else {
                reed_solomon_simd::decode(
                    header.scheme.0 as usize,
                    header.scheme.1 as usize,
                    shards.0,
                    shards.1,
                )
                .unwrap()
                .into_iter()
                .collect()
            };
            v.sort_unstable();
            let data: Vec<u8> = v.into_iter().map(|x| x.1).flatten().collect();
            let mut data = Bytes::from(data);
            let a = data.split_to(data.len() - header.padding as usize);
            return Ok(a);
        }
    }
    for (k, v) in decoded_map {
        println!("k {k}");
        assert!(v.len() <= 1280);
    }
    panic!("ah")
}

async fn disassemble(
    send_stream: &mut SendStream,
    mut bytes: Vec<u8>,
) -> Result<(), anyhow::Error> {
    let len_bytes = bytes.len();
    let chunk_size = 1280;
    let padding = chunk_size - bytes.len() % chunk_size;
    bytes.resize(chunk_size * bytes.len().div_ceil(chunk_size), 0);
    assert!(bytes.len() % chunk_size == 0);
    let data_shards = std::cmp::max(1, bytes.len() / chunk_size);
    let parity_shards = std::cmp::max(1, data_shards / 2);
    println!("data len {} {data_shards}:{parity_shards}", bytes.len(),);
    bytes.resize(data_shards * chunk_size, 0);
    let shards: Vec<_> = bytes.chunks(chunk_size).collect();
    let parity = reed_solomon_simd::encode(data_shards, parity_shards, &shards).unwrap();
    let mut ecs: Vec<Bytes> = shards
        .into_iter()
        .chain(parity.iter().map(|x| x.as_slice()))
        .map(|x| {
            let mut b = BytesMut::new();
            b.extend_from_slice(x);
            b.put_u32(len_bytes as u32);
            b.put_u32(data_shards as u32);
            b.put_u32(parity_shards as u32);
            b.put_u32(padding as u32);
            b.freeze()
        })
        .collect();
    send_stream.write_chunks(&mut ecs).await?;
    Ok(())
}

pub(crate) async fn read_request(recv_stream: RecvStream) -> Result<Request<Body>, anyhow::Error> {
    let raw_msg = assemble(recv_stream)
        .await
        .with_context(|| "Failed to read request from the stream.")?;

    let msg: WireRequest = bincode_config()
        .deserialize(&raw_msg)
        .with_context(|| "Failed to deserialize the request from the wire.")?;

    let mut request = Request::new(Body::from(Bytes::copy_from_slice(msg.body)));
    let _ = std::mem::replace(request.uri_mut(), msg.uri);
    Ok(request)
}

pub(crate) async fn read_response(
    recv_stream: RecvStream,
) -> Result<Response<Bytes>, anyhow::Error> {
    let raw_msg = assemble(recv_stream)
        .await
        .with_context(|| "Failed to read response from the stream.")?;

    let msg: WireResponse = bincode_config()
        .deserialize(&raw_msg)
        .with_context(|| "Failed to deserialize response.")?;

    let mut response = Response::new(Bytes::copy_from_slice(msg.body));
    let _ = std::mem::replace(response.status_mut(), msg.status);
    Ok(response)
}

pub(crate) async fn write_request(
    send_stream: &mut SendStream,
    request: Request<Bytes>,
) -> Result<(), anyhow::Error> {
    let (parts, body) = request.into_parts();

    let msg = WireRequest {
        uri: parts.uri,
        body: &body,
    };

    let res = bincode_config()
        .serialize(&msg)
        .with_context(|| "Failed to serialize request.")?;
    disassemble(send_stream, res).await
}

pub(crate) async fn write_response(
    send_stream: &mut SendStream,
    response: Response<Body>,
) -> Result<(), anyhow::Error> {
    let (parts, body) = response.into_parts();
    // Check for axum error in body
    // TODO: Think about this. What is the error that can happen here?
    let b = axum::body::to_bytes(body, MAX_MESSAGE_SIZE_BYTES)
        .await
        .with_context(|| "Failed to convert response body to bytes.")?;
    let msg = WireResponse {
        status: parts.status,
        body: &b,
    };
    let res = bincode_config()
        .serialize(&msg)
        .with_context(|| "Failed to serialize response.")?;
    disassemble(send_stream, res).await
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
