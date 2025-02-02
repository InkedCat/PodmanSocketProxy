use crate::errors::ReadCompleteError;
use crate::responses::{close_response, ClientReponse};
use crate::{config, responses};

use config::Config;
use httparse::Request;
use responses::{BAD_REQUEST, FORBIDDEN, NOT_ALLOWED};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc::Sender;

const MAX_REQUEST_SIZE: usize = 10 * 1024 * 1024; // 10MB

enum FilterResult {
    Allowed,
    MethodNotAllowed,
    Forbidden,
    BadRequest,
}

fn is_action_allowed(req: &Request, config: &Config) -> FilterResult {
    let method = match req.method {
        Some(method) => method,
        None => return FilterResult::MethodNotAllowed,
    };

    let path = match req.path {
        Some(path) => path,
        None => return FilterResult::Forbidden,
    };

    let proxy = match method {
        "GET" => &config.get,
        "HEAD" => &config.head,
        "POST" => &config.post,
        "PUT" => &config.put,
        "PATCH" => &config.patch,
        "DELETE" => &config.delete,
        _ => return FilterResult::BadRequest,
    };

    if proxy.allowed {
        let reg = match regex::Regex::new(&proxy.regex) {
            Ok(regex) => regex,
            Err(_) => {
                panic!("Invalid regex syntax: {}", &proxy.regex)
            }
        };

        if reg.is_match(path) {
            return FilterResult::Allowed;
        }
    }

    return FilterResult::Forbidden;
}

fn headers_contains_upgrade(headers: &[httparse::Header]) -> bool {
    for header in headers {
        if header.name.to_lowercase() == "connection" {
            return true;
        }
    }

    false
}

async fn read_request(
    buffer_reader: &mut BufReader<OwnedReadHalf>,
) -> Result<Vec<u8>, ReadCompleteError> {
    let mut buffer: Vec<u8> = Vec::with_capacity(1024 * 1024);
    let mut is_valid = false;

    let mut temp_buffer: Vec<u8> = Vec::with_capacity(1024 * 1024);
    while !is_valid {
        let size = buffer_reader.read_buf(&mut temp_buffer).await?;
        if size == 0 {
            return Err(ReadCompleteError::NoData());
        }

        if buffer.len() + size > MAX_REQUEST_SIZE {
            return Err(ReadCompleteError::ExceededMaxSize());
        }

        buffer.append(&mut temp_buffer);

        let status = httparse::Request::new(&mut [httparse::EMPTY_HEADER; 64]).parse(&buffer)?;

        is_valid = status.is_complete();
    }

    Ok(buffer)
}

// Warning - Limitation
// No support for HTTP/2 Portocol Switching, too risky to implement
pub async fn handle_client(
    protected_read: OwnedReadHalf,
    mut podman_write: OwnedWriteHalf,
    writer_channel: Sender<ClientReponse>,
    config: Config,
) -> anyhow::Result<()> {
    let mut protected_reader = BufReader::new(protected_read);

    loop {
        let request_buffer = match read_request(&mut protected_reader).await {
            Ok(buffer) => buffer,
            Err(ReadCompleteError::NoData()) => break,
            Err(ReadCompleteError::ReadError(_)) => break,
            Err(_) => {
                writer_channel.send(close_response(BAD_REQUEST)).await?;
                break;
            }
        };

        println!("Request: {:?}", String::from_utf8_lossy(&request_buffer));

        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);

        let result = req.parse(&request_buffer)?;
        if result.is_partial() {
            writer_channel.send(close_response(BAD_REQUEST)).await?;
            break;
        }

        if headers_contains_upgrade(req.headers) {
            writer_channel.send(close_response(FORBIDDEN)).await?;
            break;
        }

        match is_action_allowed(&req, &config) {
            FilterResult::Allowed => {
                podman_write.write_all(&request_buffer).await?;
            }
            FilterResult::MethodNotAllowed => {
                writer_channel.send(close_response(NOT_ALLOWED)).await?;
                break;
            }
            FilterResult::Forbidden => {
                writer_channel.send(close_response(FORBIDDEN)).await?;
                break;
            }
            FilterResult::BadRequest => {
                writer_channel.send(close_response(BAD_REQUEST)).await?;
                break;
            }
        }
    }

    Ok(())
}
