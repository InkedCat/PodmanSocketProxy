pub const BAD_REQUEST: &str = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n400 bad request";
pub const NOT_ALLOWED: &str = "HTTP/1.1 405 Method Not Allowed\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n405 method not allowed";
pub const FORBIDDEN: &str = "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\nblocked by proxy";

pub struct ClientReponse {
    pub buffer: Vec<u8>,
    pub close: bool,
}

pub fn request_response(buffer: Vec<u8>) -> ClientReponse {
    ClientReponse {
        buffer,
        close: false,
    }
}

pub fn close_response(message: &str) -> ClientReponse {
    ClientReponse {
        buffer: message.as_bytes().to_vec(),
        close: true,
    }
}
