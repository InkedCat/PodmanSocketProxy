use tokio::{io::AsyncWriteExt, net::unix::OwnedWriteHalf, sync::mpsc::Sender};

use crate::{
    errors::ReadCompleteError,
    filter::{FilterResult, FiltersHandler},
    responses::{close_response, ClientReponse, BAD_REQUEST, FORBIDDEN, NOT_ALLOWED},
};

use super::ProxyBufferedRead;

const MAX_REQUEST_SIZE: usize = 10 * 1024 * 1024; // 10MB

async fn read_request(proxy_reader: &mut ProxyBufferedRead) -> Result<Vec<u8>, ReadCompleteError> {
    let mut buffer: Vec<u8> = Vec::with_capacity(1024 * 1024);
    let mut is_valid = false;

    let mut temp_buffer: Vec<u8> = Vec::with_capacity(1024 * 1024);
    while !is_valid {
        let size = proxy_reader.read(&mut temp_buffer).await?;
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
    mut proxy_reader: ProxyBufferedRead,
    mut podman_write: OwnedWriteHalf,
    writer_channel: Sender<ClientReponse>,
    filters_handler: FiltersHandler,
) -> anyhow::Result<()> {
    loop {
        let filters_handler = filters_handler.clone();

        let request_buffer = match read_request(&mut proxy_reader).await {
            Ok(buffer) => buffer,
            Err(ReadCompleteError::NoData()) => break,
            Err(ReadCompleteError::ReadError(_)) => break,
            Err(_) => {
                writer_channel.send(close_response(BAD_REQUEST)).await?;
                debug!("Bad request");
                break;
            }
        };

        debug!("Received request: {:?}", String::from_utf8_lossy(&request_buffer));
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);

        let result = req.parse(&request_buffer)?;
        if result.is_partial() {
            writer_channel.send(close_response(BAD_REQUEST)).await?;
            debug!("Bad request");
            break;
        }

        match filters_handler.is_action_allowed(&req, &req.headers) {
            FilterResult::Allowed => {
                podman_write.write_all(&request_buffer).await?;
                debug!("Request sent to Podman");
            }
            FilterResult::MethodNotAllowed => {
                writer_channel.send(close_response(NOT_ALLOWED)).await?;
                debug!("Method not allowed");
            }
            FilterResult::Forbidden => {
                writer_channel.send(close_response(FORBIDDEN)).await?;
                debug!("Forbidden");
            }
            FilterResult::BadRequest => {
                writer_channel.send(close_response(BAD_REQUEST)).await?;
                debug!("Bad request");
            }
        }
    }

    Ok(())
}
