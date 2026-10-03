//! Token classification and the secret wrapper that keeps tokens out of debug output.

use std::fmt;

use crate::error::ConfigError;

/// Shown instead of a token or secret in debug output.
pub(crate) const REDACTED: &str = "[redacted]";

/// The kind of an API token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TokenKind {
    /// A project sending token (`lm_…`), sent as `x-lettermint-token`. Used by `emails()`.
    Sending,
    /// A team API token (`lm_team_…`), sent as `Authorization: Bearer`. Used by the Team API.
    Team,
}

impl TokenKind {
    /// Classifies a token by its format. `lm_team_` followed by letters and digits is a team
    /// token (`ApiToken::TEAM_PREFIX` in the backend). It is checked first, because every team
    /// token also starts with `lm_`. `lm_` followed by letters and digits is a sending token
    /// (`ApiToken::PROJECT_PREFIX`). Any other value, such as an SSO token (`lm_sso_…`), an
    /// OAuth token or an empty string, is a [`ConfigError`]. The error never contains the token.
    pub fn detect(token: &str) -> Result<Self, ConfigError> {
        if alphanumeric_after(token, "lm_team_") {
            Ok(Self::Team)
        } else if alphanumeric_after(token, "lm_") {
            Ok(Self::Sending)
        } else {
            Err(ConfigError::new(
                "Unrecognised token format; pass it with sending_token() or team_token() instead.",
            ))
        }
    }
}

fn alphanumeric_after(token: &str, prefix: &str) -> bool {
    token.strip_prefix(prefix).is_some_and(|rest| {
        !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_alphanumeric())
    })
}

/// Validates an explicitly configured token: non-empty and valid in an HTTP header.
pub(crate) fn check_token(option: &str, token: String) -> Result<Secret, ConfigError> {
    if token.is_empty() {
        return Err(ConfigError::new(format!("`{option}` must not be empty.")));
    }
    if !token.bytes().all(|byte| (0x21..=0x7e).contains(&byte)) {
        return Err(ConfigError::new(format!(
            "`{option}` contains whitespace or characters that are not allowed in an HTTP header."
        )));
    }
    Ok(Secret(token))
}

/// A token or secret. Its `Debug` output is `[redacted]`.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Secret(pub(crate) String);

impl Secret {
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_team_tokens_first() {
        assert_eq!(
            TokenKind::detect("lm_team_abc123").unwrap(),
            TokenKind::Team
        );
        assert_eq!(TokenKind::detect("lm_abc123").unwrap(), TokenKind::Sending);
        assert_eq!(
            TokenKind::detect("lm_Proj22Conformance0Toke").unwrap(),
            TokenKind::Sending
        );
    }

    #[test]
    fn rejects_other_formats_without_echoing_them() {
        for token in [
            "",
            "lm_",
            "lm_team_",
            "lm_sso_abc",
            "lm_team_abc-def",
            "eyJhbGciOiJIUzI1NiJ9.e30.c2ln",
            "xk_unknown_abc",
            " lm_abc",
            "lm_abc\n",
            "lm_ab€",
        ] {
            let error = TokenKind::detect(token).unwrap_err();
            assert!(error.message().starts_with("Unrecognised token format"));
            if !token.is_empty() {
                assert!(!error.message().contains(token));
            }
        }
    }

    #[test]
    fn explicit_tokens_must_be_header_safe() {
        assert!(check_token("sending_token", "anything-goes_123".into()).is_ok());
        assert!(check_token("sending_token", String::new()).is_err());
        assert!(check_token("team_token", "has space".into()).is_err());
        assert!(check_token("team_token", "line\nbreak".into()).is_err());
        let error = check_token("team_token", "nön-ascii".into()).unwrap_err();
        assert!(!error.message().contains("nön-ascii"));
    }

    #[test]
    fn secrets_are_redacted_in_debug_output() {
        let secret = Secret("lm_secret".into());
        assert_eq!(format!("{secret:?}"), "[redacted]");
        assert_eq!(format!("{secret:#?}"), "[redacted]");
    }
}
