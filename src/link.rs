//! Relative link targets made absolute against a base address, so a library user can make them clickable.
use std::borrow::Cow;
use std::fmt;

use crate::sanitize;

/// The absolute `http` or `https` URL a relative link target resolves against, as a browser resolves the links of a
/// page at that address: `/a` from its host, `a`, `./a` and `../a` from its folder, `?q` and `#f` on the page itself,
/// `//host/a` with its scheme. Its fragment is dropped.
///
/// Held inline rather than as a `String`, so `Settings` stays `Copy`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LinkBase {
    bytes: [u8; sanitize::MAX_LINK_BYTES],
    len: usize,
    origin_end: usize,
    path_end: usize,
}

/// Why a base was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LinkBaseError {
    /// Longer than the longest link `sanitize::link` accepts, 2,048 bytes.
    TooLong,
    /// Holds a character outside printable ASCII: a space, a control, or a letter a URL percent-encodes.
    Unprintable,
    /// Not an absolute `http` or `https` URL.
    NotHttp,
    /// Names no host, puts credentials before it, or holds a backslash in it.
    BadHost,
}

impl fmt::Display for LinkBaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong => write!(formatter, "the link base is longer than {} bytes", sanitize::MAX_LINK_BYTES),
            Self::Unprintable => formatter.write_str("the link base holds a character outside printable ASCII"),
            Self::NotHttp => formatter.write_str("the link base is not an absolute http or https URL"),
            Self::BadHost => formatter.write_str("the link base names no plain host"),
        }
    }
}

impl std::error::Error for LinkBaseError {}

impl LinkBase {
    pub fn new(url: &str) -> Result<Self, LinkBaseError> {
        let url = url.split_once('#').map_or(url, |(page, _)| page);
        if url.len() > sanitize::MAX_LINK_BYTES {
            return Err(LinkBaseError::TooLong);
        }
        if !url.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err(LinkBaseError::Unprintable);
        }
        let scheme = sanitize::scheme(url).filter(|scheme| is_http(scheme)).ok_or(LinkBaseError::NotHttp)?;
        let after_slashes = url[scheme.len() + 1..].strip_prefix("//").ok_or(LinkBaseError::NotHttp)?;
        let host = &after_slashes[..after_slashes.find(['/', '?']).unwrap_or(after_slashes.len())];
        if host.is_empty() || host.contains(['@', '\\']) {
            return Err(LinkBaseError::BadHost);
        }
        let origin_end = url.len() - after_slashes.len() + host.len();
        let path_end = url.find('?').unwrap_or(url.len());
        let mut bytes = [0; sanitize::MAX_LINK_BYTES];
        bytes[..url.len()].copy_from_slice(url.as_bytes());
        Ok(Self { bytes, len: url.len(), origin_end, path_end })
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).unwrap_or_default()
    }

    /// `target` made absolute, or as written when it already has a scheme, which `sanitize::link` then judges.
    pub(crate) fn resolve<'a>(&self, target: &'a str) -> Cow<'a, str> {
        if sanitize::scheme(target).is_some() {
            return Cow::Borrowed(target);
        }
        let base = self.as_str();
        let (page, query) = base.split_at(self.path_end);
        let (origin, path) = page.split_at(self.origin_end);
        let path = if path.is_empty() { "/" } else { path };
        Cow::Owned(match target.as_bytes() {
            [b'/', b'/', ..] => format!("{}:{target}", sanitize::scheme(base).unwrap_or_default()),
            [b'?', ..] => format!("{origin}{path}{target}"),
            [b'#', ..] => format!("{origin}{path}{query}{target}"),
            [b'/', ..] => format!("{origin}{}", without_dot_segments(target)),
            _ => format!("{origin}{}", without_dot_segments(&format!("{}{target}", folder(path)))),
        })
    }
}

impl fmt::Debug for LinkBase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("LinkBase").field(&self.as_str()).finish()
    }
}

fn is_http(scheme: &str) -> bool {
    scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
}

fn folder(path: &str) -> &str {
    &path[..=path.rfind('/').unwrap_or(0)]
}

/// An absolute path with its `.` and `..` segments resolved (RFC 3986 § 5.2.4), its query and fragment kept as written.
fn without_dot_segments(target: &str) -> String {
    let (path, rest) = target.split_at(target.find(['?', '#']).unwrap_or(target.len()));
    let mut kept = Vec::new();
    for segment in path.split('/').skip(1) {
        match segment {
            "." => {}
            ".." => {
                kept.pop();
            }
            _ => kept.push(segment),
        }
    }
    if matches!(path.rsplit('/').next(), Some("." | "..")) {
        kept.push("");
    }
    format!("/{}{rest}", kept.join("/"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    const BASE: &str = "https://gitlab.example.com/group/project/-/merge_requests/7?tab=notes#note_1";

    fn resolved(target: &str) -> String {
        LinkBase::new(BASE).unwrap().resolve(target).into_owned()
    }

    #[test]
    fn a_root_relative_target_starts_at_the_host() {
        assert_eq!(resolved("/uploads/ab12/report.pdf"), "https://gitlab.example.com/uploads/ab12/report.pdf");
    }

    #[test]
    fn a_relative_target_starts_in_the_page_folder() {
        assert_eq!(resolved("docs/x.md"), "https://gitlab.example.com/group/project/-/merge_requests/docs/x.md");
        assert_eq!(resolved("./docs/x.md"), "https://gitlab.example.com/group/project/-/merge_requests/docs/x.md");
    }

    #[test]
    fn dot_dot_climbs_folders_but_never_above_the_host() {
        assert_eq!(resolved("../../blob/main/a.rs#L3"), "https://gitlab.example.com/group/project/blob/main/a.rs#L3");
        assert_eq!(resolved("/a/../../../b"), "https://gitlab.example.com/b");
        assert_eq!(resolved(".."), "https://gitlab.example.com/group/project/-/");
    }

    #[test]
    fn dot_segments_in_the_query_or_fragment_stay() {
        assert_eq!(resolved("/a/./b?x=../y#./z"), "https://gitlab.example.com/a/b?x=../y#./z");
    }

    #[test]
    fn a_query_or_fragment_alone_stays_on_the_page() {
        assert_eq!(resolved("?tab=diffs"), "https://gitlab.example.com/group/project/-/merge_requests/7?tab=diffs");
        assert_eq!(resolved("#note_2"), "https://gitlab.example.com/group/project/-/merge_requests/7?tab=notes#note_2");
    }

    #[test]
    fn a_scheme_relative_target_takes_the_base_scheme() {
        assert_eq!(resolved("//cdn.example.org/a.png"), "https://cdn.example.org/a.png");
    }

    #[test]
    fn a_bare_host_reads_as_its_root() {
        let base = LinkBase::new("http://forge.test").unwrap();

        assert_eq!(base.resolve("a.md"), "http://forge.test/a.md");
        assert_eq!(base.resolve("#top"), "http://forge.test/#top");
    }

    #[test]
    fn a_target_with_a_scheme_is_left_as_written() {
        for target in ["https://other.test/a", "mailto:me@x.y", "javascript:alert(1)", "data:text/html,<b>"] {
            assert_eq!(resolved(target), target);
        }
    }

    #[test]
    fn the_base_drops_its_fragment() {
        assert_eq!(LinkBase::new(BASE).unwrap().as_str(), "https://gitlab.example.com/group/project/-/merge_requests/7?tab=notes");
    }

    #[test]
    fn a_base_that_is_not_a_plain_http_address_is_refused() {
        let cases = [
            ("javascript:alert(1)", LinkBaseError::NotHttp),
            ("data:text/html,x", LinkBaseError::NotHttp),
            ("file:///etc/", LinkBaseError::NotHttp),
            ("ftp://host/", LinkBaseError::NotHttp),
            ("/relative/path", LinkBaseError::NotHttp),
            ("https:/host", LinkBaseError::NotHttp),
            ("https:///path", LinkBaseError::BadHost),
            ("https://user:pw@host/", LinkBaseError::BadHost),
            ("https://host\\evil/", LinkBaseError::BadHost),
            ("https://host/a\x1b]52;c;x\x07", LinkBaseError::Unprintable),
            ("https://host/a\nb", LinkBaseError::Unprintable),
            ("https://host/a b", LinkBaseError::Unprintable),
            ("https://hôst/", LinkBaseError::Unprintable),
        ];

        for (base, error) in cases {
            assert_eq!(LinkBase::new(base), Err(error), "{base:?}");
        }
    }

    #[test]
    fn a_base_over_the_link_limit_is_refused() {
        let at_limit = format!("https://h/{}", "a".repeat(sanitize::MAX_LINK_BYTES - "https://h/".len()));

        assert!(LinkBase::new(&at_limit).is_ok());
        assert_eq!(LinkBase::new(&format!("{at_limit}a")), Err(LinkBaseError::TooLong));
    }
}
