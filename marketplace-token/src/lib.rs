// Copyright © The Daybrite Project
// SPDX-License-Identifier: MPL-2.0

//! Install verification tokens for App Fair's alternative app marketplace.
//!
//! MarketplaceKit refuses to install an app unless the marketplace vouches for that one install
//! with a JSON Web Signature: an ES256-signed JWT whose claims name the marketplace (`iss`), the
//! app (`bid`, and optionally `iid`/`vid`), the audience Apple expects, a fresh nonce, and a short
//! lifetime. The token is signed with the marketplace's *alternative distribution private key*,
//! which never leaves the marketplace server, so the app asks the server for one per install
//! (docs/marketplace-server.md) and this crate is what that server runs.
//!
//! The claim names and values follow Apple's "Supplying an install verification token"
//! (https://developer.apple.com/documentation/marketplacekit/supplying-an-install-verification-token).
//!
//! ```
//! use app_fair_marketplace_token::{Claims, DownloadType, Signer};
//! # let pem = app_fair_marketplace_token::testing::pkcs8_pem();
//! let signer = Signer::from_pem(&pem, "52c5cb04-1163-4a30-ad4f-a3433cd6a4f6").unwrap();
//! let claims = Claims::new("6448367041", "org.appfair.app.Net-Skip", DownloadType::Download)
//!     .item(1640618584)
//!     .version(869899373);
//! let token = signer.sign(&claims, 1_700_000_000).unwrap();
//! assert_eq!(token.split('.').count(), 3);
//! ```

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use p256::ecdsa::signature::{Signer as _, Verifier as _};
use p256::ecdsa::{Signature, SigningKey, VerifyingKey};
use p256::pkcs8::{DecodePrivateKey, DecodePublicKey};
use serde::{Deserialize, Serialize};

/// The audience every install verification token carries.
pub const AUDIENCE: &str = "AppleDownloadVerification-v1";

/// How long a token stays valid by default, in seconds. The device redeems it within moments of
/// the person confirming the install; a short window limits what a leaked token is worth.
pub const DEFAULT_LIFETIME: i64 = 20 * 60;

/// Whether this device has had the app before. Apple passes the value on to AdAttributionKit, so
/// the server has to track it per account and app rather than guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadType {
    /// The first install of the app for this account.
    Download,
    /// Any later install: a reinstall, a restore, or an update.
    Redownload,
}

/// The JWS protected header.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub alg: String,
    /// The alternative distribution key's id: the `data.id` App Store Connect returns for it.
    pub kid: String,
    pub typ: String,
}

/// A token's claims, in Apple's names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    /// The marketplace's own Apple item id, as a string.
    pub iss: String,
    /// Issued at, in Unix seconds. Filled by [`Signer::sign`].
    #[serde(default)]
    pub iat: i64,
    /// Expiry, in Unix seconds. Filled by [`Signer::sign`] from [`Claims::lifetime`].
    #[serde(default)]
    pub exp: i64,
    pub aud: String,
    /// The bundle identifier of the app being installed.
    pub bid: String,
    pub dtype: DownloadType,
    /// A random UUID, so a token is never valid twice.
    pub nonce: String,
    /// The installed app's Apple item id. Optional; it speeds up the install.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iid: Option<String>,
    /// The installed version's Apple version id. Optional; it speeds up the install.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vid: Option<String>,
    /// Seconds between `iat` and `exp`. Not a claim.
    #[serde(skip)]
    lifetime: i64,
}

impl Claims {
    /// Claims for installing `bundle_id` from the marketplace whose Apple item id is `issuer`,
    /// with a fresh nonce and the default lifetime.
    pub fn new(
        issuer: impl Into<String>,
        bundle_id: impl Into<String>,
        dtype: DownloadType,
    ) -> Self {
        Claims {
            iss: issuer.into(),
            iat: 0,
            exp: 0,
            aud: AUDIENCE.to_string(),
            bid: bundle_id.into(),
            dtype,
            nonce: uuid::Uuid::new_v4().to_string().to_uppercase(),
            iid: None,
            vid: None,
            lifetime: DEFAULT_LIFETIME,
        }
    }

    /// Name the app's Apple item id (`iid`).
    pub fn item(mut self, apple_item_id: u64) -> Self {
        self.iid = Some(apple_item_id.to_string());
        self
    }

    /// Name the version's Apple version id (`vid`).
    pub fn version(mut self, apple_version_id: u64) -> Self {
        self.vid = Some(apple_version_id.to_string());
        self
    }

    /// Use `seconds` instead of [`DEFAULT_LIFETIME`].
    pub fn lifetime(mut self, seconds: i64) -> Self {
        self.lifetime = seconds;
        self
    }

    /// Use a fixed nonce, for reproducible output in tests.
    pub fn nonce(mut self, nonce: impl Into<String>) -> Self {
        self.nonce = nonce.into();
        self
    }
}

/// Why a token could not be made or checked.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// The key is not a P-256 private key in PKCS#8 or SEC1 PEM, or not a public key in SPKI PEM.
    Key(String),
    /// A claim is missing or out of range.
    Claims(String),
    /// The token is not three base64url segments of JSON.
    Malformed(String),
    /// The signature does not verify against the key.
    Signature,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Key(e) => write!(f, "key: {e}"),
            Error::Claims(e) => write!(f, "claims: {e}"),
            Error::Malformed(e) => write!(f, "malformed token: {e}"),
            Error::Signature => f.write_str("the signature does not verify"),
        }
    }
}

impl std::error::Error for Error {}

/// Signs tokens with the marketplace's alternative distribution private key.
pub struct Signer {
    key: SigningKey,
    kid: String,
}

impl Signer {
    /// A signer for `pem` (PKCS#8 `PRIVATE KEY` or SEC1 `EC PRIVATE KEY`) whose App Store Connect
    /// key id is `kid`.
    pub fn from_pem(pem: &str, kid: impl Into<String>) -> Result<Self, Error> {
        let key = SigningKey::from_pkcs8_pem(pem)
            .or_else(|_| p256::SecretKey::from_sec1_pem(pem).map(SigningKey::from))
            .map_err(|e| Error::Key(e.to_string()))?;
        Ok(Signer {
            key,
            kid: kid.into(),
        })
    }

    /// A signer over an already-decoded key.
    pub fn new(key: SigningKey, kid: impl Into<String>) -> Self {
        Signer {
            key,
            kid: kid.into(),
        }
    }

    /// The public half, which App Store Connect holds for this key id.
    pub fn verifying_key(&self) -> VerifyingKey {
        *self.key.verifying_key()
    }

    /// Sign `claims` as issued at `now` (Unix seconds): `header.payload.signature`, each segment
    /// base64url without padding, the signature the 64-byte `r || s` JWS uses for ES256.
    pub fn sign(&self, claims: &Claims, now: i64) -> Result<String, Error> {
        validate(claims)?;
        let mut claims = claims.clone();
        claims.iat = now;
        claims.exp = now + claims.lifetime;
        let header = Header {
            alg: "ES256".into(),
            kid: self.kid.clone(),
            typ: "JWT".into(),
        };
        self.jws(&header, &claims)
    }

    /// The token a developer uploads to App Store Connect to let this marketplace distribute their
    /// apps (Apple's "Creating keys and establishing alternative marketplace connections"):
    /// `iss` the marketplace app's Apple ID, `pid` the developer's Developer ID, `aud`
    /// `appstoreconnect-v1`, valid for `lifetime` seconds from `now`, which must be under 7 days.
    pub fn marketplace_token(
        &self,
        marketplace_item_id: &str,
        developer_id: &str,
        now: i64,
        lifetime: i64,
    ) -> Result<String, Error> {
        if marketplace_item_id.is_empty()
            || !marketplace_item_id.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(Error::Claims(
                "iss must be the marketplace app's numeric Apple ID".into(),
            ));
        }
        if developer_id.trim().is_empty() {
            return Err(Error::Claims(
                "pid must be the developer's Developer ID".into(),
            ));
        }
        if lifetime <= 0 || lifetime >= MARKETPLACE_TOKEN_MAX_LIFETIME {
            return Err(Error::Claims(
                "a marketplace token must expire in less than 7 days".into(),
            ));
        }
        let header = MarketplaceHeader {
            alg: "ES256".into(),
            typ: "JWT".into(),
        };
        let claims = MarketplaceClaims {
            iss: marketplace_item_id.to_string(),
            iat: now,
            exp: now + lifetime,
            aud: MARKETPLACE_AUDIENCE.to_string(),
            pid: developer_id.trim().to_string(),
        };
        self.jws(&header, &claims)
    }

    fn jws(&self, header: &impl Serialize, claims: &impl Serialize) -> Result<String, Error> {
        let signing_input = format!("{}.{}", segment(header)?, segment(claims)?);
        let signature: Signature = self.key.sign(signing_input.as_bytes());
        Ok(format!(
            "{signing_input}.{}",
            URL_SAFE_NO_PAD.encode(signature.to_bytes())
        ))
    }
}

/// The audience of a marketplace token: App Store Connect.
pub const MARKETPLACE_AUDIENCE: &str = "appstoreconnect-v1";

/// Apple requires a marketplace token to expire in less than 7 days.
pub const MARKETPLACE_TOKEN_MAX_LIFETIME: i64 = 7 * 24 * 60 * 60;

/// A marketplace token's header: Apple's example carries no key id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketplaceHeader {
    pub alg: String,
    pub typ: String,
}

/// A marketplace token's claims.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketplaceClaims {
    pub iss: String,
    pub iat: i64,
    pub exp: i64,
    pub aud: String,
    /// The developer's Developer ID (App Store Connect → Edit Profile).
    pub pid: String,
}

fn validate(claims: &Claims) -> Result<(), Error> {
    if claims.iss.is_empty() || !claims.iss.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::Claims(
            "iss must be the marketplace's numeric Apple item id".into(),
        ));
    }
    if claims.bid.is_empty() {
        return Err(Error::Claims("bid must name the app's bundle id".into()));
    }
    if claims.lifetime <= 0 {
        return Err(Error::Claims("the lifetime must be positive".into()));
    }
    if claims.aud != AUDIENCE {
        return Err(Error::Claims(format!("aud must be {AUDIENCE}")));
    }
    Ok(())
}

fn segment(value: &impl Serialize) -> Result<String, Error> {
    let json = serde_json::to_vec(value).map_err(|e| Error::Claims(e.to_string()))?;
    Ok(URL_SAFE_NO_PAD.encode(json))
}

/// A verified token's contents.
#[derive(Debug, PartialEq, Eq)]
pub struct Verified {
    pub header: Header,
    pub claims: Claims,
}

/// Check `token`'s ES256 signature against `public_pem` (SPKI `PUBLIC KEY`) and that it is
/// unexpired at `now`, and return what it says. What the device itself checks; useful in a
/// server's tests and when diagnosing a refused install.
pub fn verify(token: &str, public_pem: &str, now: i64) -> Result<Verified, Error> {
    let key =
        VerifyingKey::from_public_key_pem(public_pem).map_err(|e| Error::Key(e.to_string()))?;
    let mut parts = token.split('.');
    let (Some(h), Some(p), Some(s), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(Error::Malformed("expected three segments".into()));
    };
    let decode = |seg: &str| {
        URL_SAFE_NO_PAD
            .decode(seg)
            .map_err(|e| Error::Malformed(e.to_string()))
    };
    let signature =
        Signature::from_slice(&decode(s)?).map_err(|e| Error::Malformed(e.to_string()))?;
    key.verify(format!("{h}.{p}").as_bytes(), &signature)
        .map_err(|_| Error::Signature)?;
    let header: Header =
        serde_json::from_slice(&decode(h)?).map_err(|e| Error::Malformed(e.to_string()))?;
    let claims: Claims =
        serde_json::from_slice(&decode(p)?).map_err(|e| Error::Malformed(e.to_string()))?;
    if header.alg != "ES256" {
        return Err(Error::Malformed(format!("alg {} is not ES256", header.alg)));
    }
    if claims.aud != AUDIENCE {
        return Err(Error::Claims(format!(
            "aud {} is not {AUDIENCE}",
            claims.aud
        )));
    }
    if claims.exp <= now {
        return Err(Error::Claims("the token has expired".into()));
    }
    Ok(Verified { header, claims })
}

/// A fixed key pair for documentation and tests. Never use it for a real marketplace.
#[doc(hidden)]
pub mod testing {
    use p256::ecdsa::SigningKey;
    use p256::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};

    /// A valid P-256 scalar, fixed so test output is reproducible.
    const SCALAR: [u8; 32] = [
        0x1f, 0x8e, 0x2a, 0x55, 0x03, 0x77, 0xc4, 0x9b, 0x61, 0x2d, 0xe0, 0x4a, 0x93, 0x0b, 0x6c,
        0x28, 0x5e, 0xd1, 0x47, 0xaa, 0x36, 0x80, 0x12, 0xfb, 0x09, 0xcc, 0x75, 0x4e, 0xb3, 0x2f,
        0x68, 0x91,
    ];

    pub fn key() -> SigningKey {
        SigningKey::from_slice(&SCALAR).expect("a valid P-256 scalar")
    }

    pub fn pkcs8_pem() -> String {
        key()
            .to_pkcs8_pem(LineEnding::LF)
            .expect("encodable key")
            .to_string()
    }

    pub fn sec1_pem() -> String {
        p256::SecretKey::from_slice(&SCALAR)
            .expect("a valid P-256 scalar")
            .to_sec1_pem(LineEnding::LF)
            .expect("encodable key")
            .to_string()
    }

    pub fn public_pem() -> String {
        key()
            .verifying_key()
            .to_public_key_pem(LineEnding::LF)
            .expect("encodable key")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KID: &str = "52c5cb04-1163-4a30-ad4f-a3433cd6a4f6";
    const NOW: i64 = 1_762_000_000;

    fn claims() -> Claims {
        Claims::new(
            "6448367041",
            "org.appfair.app.Net-Skip",
            DownloadType::Download,
        )
        .nonce("9BC2C5CC-A1F8-4F93-9D6A-4D524685B67E")
    }

    fn json(segment: &str) -> serde_json::Value {
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(segment).unwrap()).unwrap()
    }

    #[test]
    fn a_token_carries_exactly_apples_header_and_claims() {
        let signer = Signer::from_pem(&testing::pkcs8_pem(), KID).unwrap();
        let token = signer
            .sign(&claims().item(1640618584).version(869899373), NOW)
            .unwrap();
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(
            json(parts[0]),
            serde_json::json!({"alg": "ES256", "kid": KID, "typ": "JWT"})
        );
        assert_eq!(
            json(parts[1]),
            serde_json::json!({
                "iss": "6448367041",
                "iat": NOW,
                "exp": NOW + DEFAULT_LIFETIME,
                "aud": "AppleDownloadVerification-v1",
                "bid": "org.appfair.app.Net-Skip",
                "dtype": "download",
                "nonce": "9BC2C5CC-A1F8-4F93-9D6A-4D524685B67E",
                "iid": "1640618584",
                "vid": "869899373",
            })
        );
        // ES256 in a JWS is the raw 64-byte r || s, not DER.
        assert_eq!(URL_SAFE_NO_PAD.decode(parts[2]).unwrap().len(), 64);
    }

    #[test]
    fn optional_ids_are_left_out_rather_than_null() {
        let signer = Signer::from_pem(&testing::pkcs8_pem(), KID).unwrap();
        let token = signer.sign(&claims(), NOW).unwrap();
        let payload = json(token.split('.').nth(1).unwrap());
        assert!(payload.get("iid").is_none());
        assert!(payload.get("vid").is_none());
    }

    #[test]
    fn a_signed_token_verifies_and_round_trips() {
        let signer = Signer::from_pem(&testing::sec1_pem(), KID).unwrap();
        let claims = claims().item(1).version(2).lifetime(60);
        let token = signer.sign(&claims, NOW).unwrap();
        let verified = verify(&token, &testing::public_pem(), NOW + 59).unwrap();
        assert_eq!(verified.header.kid, KID);
        assert_eq!(verified.claims.bid, "org.appfair.app.Net-Skip");
        assert_eq!(verified.claims.exp, NOW + 60);
        assert_eq!(verified.claims.dtype, DownloadType::Download);
    }

    #[test]
    fn an_expired_or_tampered_token_is_refused() {
        let signer = Signer::from_pem(&testing::pkcs8_pem(), KID).unwrap();
        let token = signer.sign(&claims().lifetime(60), NOW).unwrap();
        assert_eq!(
            verify(&token, &testing::public_pem(), NOW + 60),
            Err(Error::Claims("the token has expired".into()))
        );

        let mut parts: Vec<String> = token.split('.').map(String::from).collect();
        let mut payload = json(&parts[1]);
        payload["bid"] = "com.example.other".into();
        parts[1] = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        assert_eq!(
            verify(&parts.join("."), &testing::public_pem(), NOW),
            Err(Error::Signature)
        );
    }

    #[test]
    fn a_marketplace_token_carries_apples_claims() {
        let signer = Signer::from_pem(&testing::pkcs8_pem(), KID).unwrap();
        let token = signer
            .marketplace_token(
                "6480579814",
                "57246542-96fe-1a63-e053-0824d011072a",
                NOW,
                86_400,
            )
            .unwrap();
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(
            json(parts[0]),
            serde_json::json!({"alg": "ES256", "typ": "JWT"})
        );
        assert_eq!(
            json(parts[1]),
            serde_json::json!({
                "iss": "6480579814",
                "iat": NOW,
                "exp": NOW + 86_400,
                "aud": "appstoreconnect-v1",
                "pid": "57246542-96fe-1a63-e053-0824d011072a",
            })
        );
        // It verifies against the public key App Store Connect holds.
        let key = VerifyingKey::from_public_key_pem(&testing::public_pem()).unwrap();
        let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap()).unwrap();
        key.verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .unwrap();
        // Seven days or more, or a missing developer, is refused.
        assert!(matches!(
            signer.marketplace_token("6480579814", "x", NOW, MARKETPLACE_TOKEN_MAX_LIFETIME),
            Err(Error::Claims(_))
        ));
        assert!(matches!(
            signer.marketplace_token("6480579814", " ", NOW, 60),
            Err(Error::Claims(_))
        ));
        assert!(matches!(
            signer.marketplace_token("App Fair", "x", NOW, 60),
            Err(Error::Claims(_))
        ));
    }

    #[test]
    fn nonces_are_fresh_uuids() {
        let a = Claims::new("1", "a.b", DownloadType::Redownload);
        let b = Claims::new("1", "a.b", DownloadType::Redownload);
        assert_ne!(a.nonce, b.nonce);
        assert!(uuid::Uuid::parse_str(&a.nonce).is_ok());
    }

    #[test]
    fn bad_claims_and_keys_are_rejected() {
        let signer = Signer::from_pem(&testing::pkcs8_pem(), KID).unwrap();
        let no_issuer = Claims::new("App Fair", "a.b", DownloadType::Download);
        assert!(matches!(
            signer.sign(&no_issuer, NOW),
            Err(Error::Claims(_))
        ));
        let no_bundle = Claims::new("1", "", DownloadType::Download);
        assert!(matches!(
            signer.sign(&no_bundle, NOW),
            Err(Error::Claims(_))
        ));
        assert!(matches!(
            Signer::from_pem("not a key", KID),
            Err(Error::Key(_))
        ));
        assert!(matches!(
            verify("a.b", &testing::public_pem(), NOW),
            Err(Error::Malformed(_))
        ));
    }
}
