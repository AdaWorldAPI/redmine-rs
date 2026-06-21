//! Seed users — dev-only, in-code credential table.
//!
//! W0.3's role is shaping the auth round-trip (login → cookie → /me).
//! DB-backed users + argon2 hashing land in W4 (Plan §3 width track,
//! User / Member / Role admin pages). Today, [`SEED_USERS`] is a
//! `&'static [SeedUser]` everybody can read; [`verify_seed`] does a
//! constant-time password compare against it.
//!
//! Note: plaintext seed passwords are **acceptable here only** because
//! these are dev credentials shipped in source; never use this table
//! shape for production users. Argon2 + DB is the right substrate
//! and W4 wires it.

use subtle::ConstantTimeEq;

/// One seed user.
#[derive(Debug, Clone, Copy)]
pub struct SeedUser {
    /// Lowercase login (Redmine + OpenProject convention).
    pub login: &'static str,
    /// Plaintext password (see module-doc warning).
    pub password: &'static str,
    /// Human-readable display name.
    pub display_name: &'static str,
}

/// The dev-user table. Two users mirroring the historical Redmine
/// and OpenProject defaults so anyone reading the README recognises
/// the credentials without lookup.
pub const SEED_USERS: &[SeedUser] = &[
    SeedUser {
        login: "admin",
        password: "admin",
        display_name: "Redmine Admin",
    },
    SeedUser {
        login: "jsmith",
        password: "jsmith",
        display_name: "John Smith",
    },
];

/// Constant-time credential check. Returns the matching `SeedUser`
/// or `None`.
///
/// Constant-time on the password compare; the login lookup itself
/// is O(n) and not constant-time (the seed table is small + the
/// usernames aren't secret).
#[must_use]
pub fn verify_seed(login: &str, password: &str) -> Option<SeedUser> {
    for user in SEED_USERS {
        if user.login == login {
            let pw_match: bool = user.password.as_bytes().ct_eq(password.as_bytes()).into();
            if pw_match {
                return Some(*user);
            }
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_seed_credentials_verify() {
        assert!(verify_seed("admin", "admin").is_some());
        assert!(verify_seed("jsmith", "jsmith").is_some());
    }

    #[test]
    fn wrong_password_does_not_verify() {
        assert!(verify_seed("admin", "WRONG").is_none());
        assert!(verify_seed("jsmith", "").is_none());
    }

    #[test]
    fn unknown_login_does_not_verify() {
        assert!(verify_seed("not-a-real-user", "admin").is_none());
        assert!(verify_seed("", "admin").is_none());
    }

    #[test]
    fn seed_users_are_dense_and_have_display_names() {
        // Defensive: every entry must have a non-empty login + display
        // name. Catches a sloppy edit (e.g. one user dropped to "").
        for u in SEED_USERS {
            assert!(!u.login.is_empty(), "empty login: {u:?}");
            assert!(!u.display_name.is_empty(), "empty display: {u:?}");
        }
    }
}
