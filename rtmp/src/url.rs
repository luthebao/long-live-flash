//! RTMP URL parser. Mirrors `src/rtmp.odin::rtmp_parse_url`.
//!
//! Accepted schemes: rtmp, rtmpe, rtmps, rtmpt, rtmpte. The `app` field is
//! the full path minus leading/trailing slashes — multi-segment apps like
//! `master/test` are kept intact, since some custom RTMP servers route on
//! the compound string.

use crate::RtmpError;

pub const DEFAULT_PORT: u16 = 1935;

#[derive(Debug, Clone)]
pub struct RtmpUrl {
    pub scheme: Scheme,
    pub host: String,
    pub port: u16,
    /// First non-empty path segment, joined with slashes if multi-segment.
    pub app: String,
    /// Full original URL as the SWF asked for it; sent verbatim in the
    /// `tcUrl` field of the AMF0 `connect` command.
    pub tc_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Rtmp,
    Rtmpe,
    Rtmps,
    Rtmpt,
    Rtmpte,
}

impl Scheme {
    pub fn as_str(self) -> &'static str {
        match self {
            Scheme::Rtmp => "rtmp",
            Scheme::Rtmpe => "rtmpe",
            Scheme::Rtmps => "rtmps",
            Scheme::Rtmpt => "rtmpt",
            Scheme::Rtmpte => "rtmpte",
        }
    }
}

pub fn parse(url: &str) -> Result<RtmpUrl, RtmpError> {
    let (scheme, default_port, rest) = if let Some(r) = url.strip_prefix("rtmpte://") {
        (Scheme::Rtmpte, 80u16, r)
    } else if let Some(r) = url.strip_prefix("rtmpt://") {
        (Scheme::Rtmpt, 80, r)
    } else if let Some(r) = url.strip_prefix("rtmpe://") {
        (Scheme::Rtmpe, DEFAULT_PORT, r)
    } else if let Some(r) = url.strip_prefix("rtmps://") {
        (Scheme::Rtmps, 443, r)
    } else if let Some(r) = url.strip_prefix("rtmp://") {
        (Scheme::Rtmp, DEFAULT_PORT, r)
    } else {
        return Err(RtmpError::BadUrl(url.into()));
    };

    let (authority, path_part) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, ""),
    };

    let (host, port) = match authority.rfind(':') {
        Some(i) => {
            let p: u16 = authority[i + 1..]
                .parse()
                .map_err(|_| RtmpError::BadUrl(url.into()))?;
            (authority[..i].to_string(), p)
        }
        None => (authority.to_string(), default_port),
    };

    if host.is_empty() {
        return Err(RtmpError::BadUrl(url.into()));
    }

    let app = path_part.trim_end_matches('/').to_string();

    Ok(RtmpUrl {
        scheme,
        host,
        port,
        app,
        tc_url: url.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain() {
        let u = parse("rtmp://host/app").unwrap();
        assert_eq!(u.scheme, Scheme::Rtmp);
        assert_eq!(u.host, "host");
        assert_eq!(u.port, 1935);
        assert_eq!(u.app, "app");
    }

    #[test]
    fn multi_segment_app() {
        let u = parse("rtmp://host:1944/master/test/").unwrap();
        assert_eq!(u.port, 1944);
        assert_eq!(u.app, "master/test");
        assert_eq!(u.tc_url, "rtmp://host:1944/master/test/");
    }

    #[test]
    fn rtmpe() {
        let u = parse("rtmpe://example.com/live").unwrap();
        assert_eq!(u.scheme, Scheme::Rtmpe);
        assert_eq!(u.port, 1935);
    }

    #[test]
    fn rejects_unknown_scheme() {
        assert!(parse("http://x/y").is_err());
        assert!(parse("rtmp://").is_err());
    }
}
