//! Relative link targets made absolute against a base address, so a library user can make them clickable.
use std::borrow::Cow;
use std::fmt;

use crate::sanitize;

/// Where a page's relative link targets resolve, as a browser resolves the links of the page at its address: `a`,
/// `./a` and `../a` from its folder, `?q` and `#f` on the page itself, `//host/a` with its scheme, and `/a` from its
/// host, or from a root set with [`LinkBase::rooted_at`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkBase {
    page: Address,
    root: Option<Root>,
}

/// Where the root-relative targets under `prefix` resolve instead of the page's host.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Root {
    prefix: String,
    address: Address,
}

/// An absolute `http` or `https` URL without its fragment, cut where its path starts and ends.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Address {
    url: String,
    origin_end: usize,
    path_end: usize,
}

/// Why a base or a root was refused.
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
    /// A root's prefix that is not a path of printable ASCII starting with `/`.
    BadPrefix,
}

impl fmt::Display for LinkBaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong => write!(formatter, "the link base is longer than {} bytes", sanitize::MAX_LINK_BYTES),
            Self::Unprintable => formatter.write_str("the link base holds a character outside printable ASCII"),
            Self::NotHttp => formatter.write_str("the link base is not an absolute http or https URL"),
            Self::BadHost => formatter.write_str("the link base names no plain host"),
            Self::BadPrefix => formatter.write_str("the link root's prefix is not a path starting with /"),
        }
    }
}

impl std::error::Error for LinkBaseError {}

impl LinkBase {
    /// The base of the page at `page_url`, an absolute `http` or `https` URL of printable ASCII; its fragment is dropped.
    pub fn new(page_url: &str) -> Result<Self, LinkBaseError> {
        Ok(Self { page: Address::new(page_url)?, root: None })
    }

    /// The base with the root-relative targets whose path starts with `prefix` resolving under `root_url` instead of
    /// the page's host: with `/uploads/` and `https://gitlab.com/group/project`, `/uploads/a.pdf` becomes
    /// `https://gitlab.com/group/project/uploads/a.pdf`; `/` roots every such target. Their `..` never climbs above
    /// the root, whose query and fragment are ignored. A later call replaces the root.
    pub fn rooted_at(self, prefix: &str, root_url: &str) -> Result<Self, LinkBaseError> {
        let is_path = prefix.starts_with('/') && prefix.bytes().all(|byte| byte.is_ascii_graphic());
        if !is_path {
            return Err(LinkBaseError::BadPrefix);
        }
        Ok(Self { root: Some(Root { prefix: prefix.to_owned(), address: Address::new(root_url)? }), ..self })
    }

    /// The page's URL, without its fragment.
    pub fn as_str(&self) -> &str {
        &self.page.url
    }

    /// `target` made absolute, or as written when it already has a scheme, which `sanitize::link` then judges.
    pub(crate) fn resolve<'a>(&self, target: &'a str) -> Cow<'a, str> {
        if sanitize::scheme(target).is_some() {
            return Cow::Borrowed(target);
        }
        let page = &self.page;
        Cow::Owned(match target.as_bytes() {
            [b'/', b'/', ..] => format!("{}:{target}", sanitize::scheme(&page.url).unwrap_or_default()),
            [b'?', ..] => format!("{}{}{target}", page.origin(), page.path()),
            [b'#', ..] => format!("{}{}{}{target}", page.origin(), page.path(), page.query()),
            [b'/', ..] => self.root_relative(&without_dot_segments(target)),
            _ => format!("{}{}", page.origin(), without_dot_segments(&format!("{}{target}", page.folder()))),
        })
    }

    fn root_relative(&self, target: &str) -> String {
        match &self.root {
            Some(root) if target.starts_with(&root.prefix) => {
                format!("{}{}{target}", root.address.origin(), root.address.path().trim_end_matches('/'))
            }
            _ => format!("{}{target}", self.page.origin()),
        }
    }
}

impl Address {
    fn new(url: &str) -> Result<Self, LinkBaseError> {
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
        Ok(Self { url: url.to_owned(), origin_end, path_end })
    }

    fn origin(&self) -> &str {
        &self.url[..self.origin_end]
    }

    /// The path, `/` for a bare host, as a browser reads it.
    fn path(&self) -> &str {
        let path = &self.url[self.origin_end..self.path_end];
        if path.is_empty() { "/" } else { path }
    }

    fn folder(&self) -> &str {
        let path = self.path();
        &path[..=path.rfind('/').unwrap_or(0)]
    }

    fn query(&self) -> &str {
        &self.url[self.path_end..]
    }
}

fn is_http(scheme: &str) -> bool {
    scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
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

    const ROOT: &str = "https://gitlab.example.com/group/project";

    fn rooted(prefix: &str, target: &str) -> String {
        LinkBase::new(BASE).unwrap().rooted_at(prefix, ROOT).unwrap().resolve(target).into_owned()
    }

    #[test]
    fn a_gitlab_upload_resolves_under_the_project() {
        assert_eq!(rooted("/uploads/", "/uploads/ab/x.pdf"), "https://gitlab.example.com/group/project/uploads/ab/x.pdf");
    }

    #[test]
    fn a_root_relative_target_outside_the_prefix_starts_at_the_page_host() {
        assert_eq!(rooted("/uploads/", "/other/group/-/issues/1"), "https://gitlab.example.com/other/group/-/issues/1");
        assert_eq!(rooted("/uploads/", "/uploads/../secret"), "https://gitlab.example.com/secret");
    }

    #[test]
    fn dot_dot_never_climbs_above_the_root() {
        assert_eq!(rooted("/", "/a/../../../b?x#y"), "https://gitlab.example.com/group/project/b?x#y");
    }

    #[test]
    fn a_root_changes_only_root_relative_targets() {
        for target in ["docs/x.md", "../y", "?tab=diffs", "#note_2", "//cdn.example.org/a", "https://other.test/a"] {
            assert_eq!(rooted("/", target), resolved(target), "{target}");
        }
    }

    #[test]
    fn a_root_may_sit_on_another_host_and_drops_its_query() {
        let base = LinkBase::new(BASE).unwrap().rooted_at("/", "https://pages.example.org/site/?v=1#top").unwrap();

        assert_eq!(base.resolve("/a.html"), "https://pages.example.org/site/a.html");
    }

    #[test]
    fn a_root_is_checked_like_a_base() {
        let base = || LinkBase::new(BASE).unwrap();

        assert_eq!(base().rooted_at("/", "javascript:alert(1)"), Err(LinkBaseError::NotHttp));
        assert_eq!(base().rooted_at("/", "https://h/\x1b]52;c;x\x07"), Err(LinkBaseError::Unprintable));
        assert_eq!(base().rooted_at("/", "https://u@h/"), Err(LinkBaseError::BadHost));
        for prefix in ["uploads/", "", "/a b", "/a\nb"] {
            assert_eq!(base().rooted_at(prefix, ROOT), Err(LinkBaseError::BadPrefix), "{prefix:?}");
        }
    }
}
