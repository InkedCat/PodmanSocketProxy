pub const BAD_REQUEST: &str = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n400 bad request";
pub const NOT_ALLOWED: &str = "HTTP/1.1 405 Method Not Allowed\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n405 method not allowed";
pub const FORBIDDEN: &str = "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\nblocked by proxy";
