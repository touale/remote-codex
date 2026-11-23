//! Remote Linux paths are POSIX paths even when the client runs on Windows.
pub fn absolute(path: &str) -> bool {
    path.starts_with('/') && !path.chars().any(char::is_control)
}

pub fn relative(path: &str) -> bool {
    !path.starts_with('/')
        && !path.contains('\0')
        && path
            .split('/')
            .all(|part| part.is_empty() || !matches!(part, "." | ".."))
}

pub fn within(path: &str, root: &str) -> bool {
    absolute(path)
        && absolute(root)
        && !path.split('/').any(|part| part == "..")
        && (path == root
            || root == "/"
            || path
                .strip_prefix(root.trim_end_matches('/'))
                .is_some_and(|tail| tail.starts_with('/')))
}

pub fn file_url(path: &str) -> Result<url::Url, url::ParseError> {
    if !absolute(path) {
        return Err(url::ParseError::RelativeUrlWithoutBase);
    }
    let mut url = url::Url::parse("file:///")?;
    url.set_path(path);
    Ok(url)
}

#[cfg(test)]
mod tests {
    #[test]
    fn remote_roots_do_not_depend_on_the_host_platform() -> Result<(), url::ParseError> {
        assert!(super::within("/workspace/a/file", "/workspace/a"));
        assert!(!super::within("/workspace/ab", "/workspace/a"));
        assert!(!super::within("/workspace/a/../b", "/workspace/a"));
        assert!(!super::absolute(r"C:\workspace"));
        assert_eq!(
            super::file_url("/workspace/a b")?.as_str(),
            "file:///workspace/a%20b"
        );
        Ok(())
    }
}
