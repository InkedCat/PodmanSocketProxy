use crate::config;

use config::Filters;
use httparse::Request;

pub enum FilterResult {
    Allowed,
    MethodNotAllowed,
    Forbidden,
    BadRequest,
}

#[derive(Clone)]
pub struct FiltersHandler {
    filters: Filters,
}

impl FiltersHandler {
    pub fn new(filters: Filters) -> Self {
        FiltersHandler { filters }
    }

    pub fn is_action_allowed(self, req: &Request, headers: &[httparse::Header]) -> FilterResult {
        if Self::is_headers_forbidden(headers) {
            return FilterResult::Forbidden;
        }

        let method = match req.method {
            Some(method) => method,
            None => return FilterResult::MethodNotAllowed,
        };

        let path = match req.path {
            Some(path) => path,
            None => return FilterResult::Forbidden,
        };

        let proxy = match method {
            "GET" => &self.filters.get,
            "HEAD" => &self.filters.head,
            "POST" => &self.filters.post,
            "PUT" => &self.filters.put,
            "PATCH" => &self.filters.patch,
            "DELETE" => &self.filters.delete,
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

    fn is_headers_forbidden(headers: &[httparse::Header]) -> bool {
        for header in headers {
            if header.name.to_lowercase() == "connection" {
                return true;
            }
        }

        false
    }
}
