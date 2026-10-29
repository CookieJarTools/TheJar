use std::cmp::Ordering;
use std::fmt;

/// A release version as written in tags and installers: `v0.2.0`, `0.4`, `1.0.0-beta.2`.
/// Missing minor or patch parts count as zero, so `0.4` and `0.4.0` are equal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version(semver::Version);

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix('v').or_else(|| text.strip_prefix('V')).unwrap_or(text);
        let split = text.find(['-', '+']).unwrap_or(text.len());
        let (core, rest) = text.split_at(split);

        let mut parts: Vec<&str> = core.split('.').collect();
        if parts.is_empty() || parts.len() > 3 {
            return None;
        }
        if parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit())) {
            return None;
        }
        while parts.len() < 3 {
            parts.push("0");
        }
        semver::Version::parse(&format!("{}{}", parts.join("."), rest)).ok().map(Version)
    }

    pub fn is_prerelease(&self) -> bool {
        !self.0.pre.is_empty()
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp_precedence(&other.0)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0.major, self.0.minor, self.0.patch)?;
        if !self.0.pre.is_empty() {
            write!(f, "-{}", self.0.pre)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Version;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn reads_tags_and_short_versions() {
        assert_eq!(v("v0.2.0").to_string(), "0.2.0");
        assert_eq!(v("0.4").to_string(), "0.4.0");
        assert_eq!(v(" V1 ").to_string(), "1.0.0");
        assert_eq!(v("1.2.3-beta.1+build.5").to_string(), "1.2.3-beta.1");
    }

    #[test]
    fn rejects_things_that_are_not_versions() {
        for bad in ["", "v", "latest", "1.2.3.4", "1..2", "1.x", "nightly-2026"] {
            assert!(Version::parse(bad).is_none(), "{bad} parsed");
        }
    }

    #[test]
    fn orders_like_semver() {
        assert!(v("0.10.0") > v("0.9.9"));
        assert!(v("1.0.0") > v("1.0.0-rc.1"));
        assert!(v("1.0.0-rc.2") > v("1.0.0-rc.1"));
        assert_eq!(v("0.4"), v("v0.4.0"));
        assert_eq!(v("1.0.0+a").cmp(&v("1.0.0+b")), std::cmp::Ordering::Equal);
    }
}
