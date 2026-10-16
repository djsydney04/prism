use base64::{engine::general_purpose::STANDARD, Engine};
use prism_openai::{
    reqwest::header::{HeaderMap, HeaderValue},
    webhooks::{WebhookError, WebhookHeaders, WebhookVerifier},
};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

const BODY: &[u8] = br#"{"type":"response.completed","data":{"id":"resp_test"}}"#;
const NOW: u64 = 1_700_000_000;

// Fixed independent vectors generated using Python's standard-library hmac,
// hashlib.sha256, and base64 modules with key bytes(range(32)). No Rust signing
// implementation is used to derive the expected tags in these tests.
const SIGNATURE: &str = "v1,Z+80NDt7u+mJ4wM35K/yT3mUwjsh3SnnwZrMX8mUG1g=";
const BINARY_SIGNATURE: &str = "v1,aixgVJ2suJbz2pv5fSNvjpEt/+OHpGwRZ/GsF2AISdQ=";
const EMPTY_SIGNATURE: &str = "v1,AxJF7D86X9JxF+joibBGMJ8L+whjRMisEIKc4SKbvYI=";

fn headers(signature: &str) -> WebhookHeaders<'_> {
    WebhookHeaders {
        id: "wh_test",
        timestamp: "1700000000",
        signature,
    }
}
// This public, deterministic fixture is never a provider-issued credential.
// Build its encoding here so secret scanners do not mistake it for a live key.
fn test_secret() -> String {
    let key: Vec<u8> = (0..32).collect();
    format!("whsec_{}", STANDARD.encode(key))
}

fn verifier() -> WebhookVerifier {
    WebhookVerifier::new(&test_secret()).unwrap()
}

#[test]
fn independent_vector_verifies_and_exposes_authenticated_metadata() {
    let verifier = verifier();
    let verified = verifier.verify_at(BODY, headers(SIGNATURE), NOW).unwrap();
    assert_eq!(verified.id(), "wh_test");
    assert_eq!(verified.timestamp(), NOW);
    assert_eq!(verified.body(), BODY);
    assert_eq!(
        verified.json::<Value>().unwrap()["type"],
        "response.completed"
    );
    assert_eq!(
        verifier
            .unwrap_at::<Value>(BODY, headers(SIGNATURE), NOW)
            .unwrap()["data"]["id"],
        "resp_test"
    );
}

#[test]
fn verification_uses_raw_bytes_including_invalid_json_and_non_utf8() {
    let verifier = verifier();
    let verified = verifier
        .verify_at(b"not-json\0\xff", headers(BINARY_SIGNATURE), NOW)
        .unwrap();
    assert_eq!(verified.body(), b"not-json\0\xff");
    assert!(matches!(
        verified.json::<Value>(),
        Err(WebhookError::Json(_))
    ));
    assert!(verifier
        .verify_at(b"", headers(EMPTY_SIGNATURE), NOW)
        .is_ok());
}

#[test]
fn every_body_byte_and_authenticated_header_is_tamper_protected() {
    let verifier = verifier();
    for index in 0..BODY.len() {
        let mut tampered = BODY.to_vec();
        tampered[index] ^= 1;
        assert!(matches!(
            verifier.verify_at(&tampered, headers(SIGNATURE), NOW),
            Err(WebhookError::InvalidSignature)
        ));
    }
    let mut altered = headers(SIGNATURE);
    altered.id = "wh_different";
    assert!(matches!(
        verifier.verify_at(BODY, altered, NOW),
        Err(WebhookError::InvalidSignature)
    ));
    altered = headers(SIGNATURE);
    altered.timestamp = "1700000001";
    assert!(matches!(
        verifier.verify_at(BODY, altered, NOW),
        Err(WebhookError::InvalidSignature)
    ));
    assert!(matches!(
        verifier.verify_at(
            b"{\"type\": \"response.completed\",\"data\":{\"id\":\"resp_test\"}}",
            headers(SIGNATURE),
            NOW
        ),
        Err(WebhookError::InvalidSignature)
    ));
}

#[test]
fn tolerance_applies_in_both_directions_and_handles_integer_boundaries() {
    let verifier = verifier();
    assert!(verifier
        .verify_at(BODY, headers(SIGNATURE), NOW + 300)
        .is_ok());
    assert!(verifier
        .verify_at(BODY, headers(SIGNATURE), NOW - 300)
        .is_ok());
    assert!(matches!(
        verifier.verify_at(BODY, headers(SIGNATURE), NOW + 301),
        Err(WebhookError::TimestampTooOld)
    ));
    assert!(matches!(
        verifier.verify_at(BODY, headers(SIGNATURE), NOW - 301),
        Err(WebhookError::TimestampInFuture)
    ));
    assert!(matches!(
        verifier.verify_at(BODY, headers(SIGNATURE), u64::MAX),
        Err(WebhookError::TimestampTooOld)
    ));
    let mut maximum = headers(SIGNATURE);
    maximum.timestamp = "18446744073709551615";
    assert!(matches!(
        verifier.verify_at(BODY, maximum, 0),
        Err(WebhookError::TimestampInFuture)
    ));
    let exact = verifier.with_tolerance(Duration::ZERO);
    assert!(exact.verify_at(BODY, headers(SIGNATURE), NOW).is_ok());
    assert!(matches!(
        exact.verify_at(BODY, headers(SIGNATURE), NOW + 1),
        Err(WebhookError::TimestampTooOld)
    ));
    assert!(matches!(
        exact.verify_at(BODY, headers(SIGNATURE), NOW - 1),
        Err(WebhookError::TimestampInFuture)
    ));
}

#[test]
fn secret_rotation_accepts_any_matching_v1_signature() {
    let verifier = verifier();
    for value in [
        format!("v1,{} {SIGNATURE}", "A".repeat(44)),
        format!("{SIGNATURE} v1,{}", "A".repeat(44)),
        format!(" v2,unsupported  v1,malformed {SIGNATURE} "),
    ] {
        assert!(verifier.verify_at(BODY, headers(&value), NOW).is_ok());
    }
    let wrong_version = SIGNATURE.replacen("v1,", "v2,", 1);
    assert!(matches!(
        verifier.verify_at(BODY, headers(&wrong_version), NOW),
        Err(WebhookError::InvalidSignature)
    ));
}

#[test]
fn malformed_signatures_timestamps_and_ids_are_rejected() {
    let verifier = verifier();
    for value in [
        "",
        " ",
        "v1",
        "v1,",
        "v1,%%",
        "v1,YQ==",
        "v1,not-base64",
        "v2,unrecognized",
        "v1,abc,extra",
        "v1\t,abc",
        "v1,abc\n",
        "🦀",
    ] {
        assert!(
            verifier.verify_at(BODY, headers(value), NOW).is_err(),
            "accepted malformed signature header"
        );
    }
    for timestamp in [
        "",
        " ",
        "+1700000000",
        "-1700000000",
        "1700000000x",
        "1.7e9",
        "1700000000\n",
        "18446744073709551616",
        "١٧٠٠٠٠٠٠٠٠",
    ] {
        let mut input = headers(SIGNATURE);
        input.timestamp = timestamp;
        assert!(matches!(
            verifier.verify_at(BODY, input, NOW),
            Err(WebhookError::InvalidTimestamp)
        ));
    }
    for id in ["", " ", "wh_test\n", "wh test", "wh_🦀"] {
        let mut input = headers(SIGNATURE);
        input.id = id;
        assert!(matches!(
            verifier.verify_at(BODY, input, NOW),
            Err(WebhookError::InvalidHeader("webhook-id"))
        ));
    }
}

#[test]
fn invalid_secrets_and_debug_output_do_not_disclose_credentials() {
    for secret in [
        "",
        "whsec_",
        "not-prefixed",
        "whsec_%%%%",
        "whsec_A",
        "whsec_YQ===",
        "whsec_YQ==\n",
    ] {
        let error = WebhookVerifier::new(secret).unwrap_err();
        assert!(matches!(error, WebhookError::InvalidSecret));
        assert_eq!(error.to_string(), "invalid webhook signing secret");
        assert_eq!(format!("{error:?}"), "InvalidSecret");
    }
    let debug = format!("{:?}", verifier());
    assert!(!debug.contains(&test_secret()));
    assert!(debug.contains("[REDACTED]"));
    assert!(!format!("{:?}", headers(SIGNATURE)).contains(SIGNATURE));
    assert!(!format!(
        "{:?}",
        verifier().verify_at(BODY, headers(SIGNATURE), NOW).unwrap()
    )
    .contains("response.completed"));
}

#[test]
fn header_map_handles_case_and_rejects_missing_duplicate_and_nontext_values() {
    let mut map = HeaderMap::new();
    map.insert("Webhook-Id", HeaderValue::from_static("wh_test"));
    map.insert("Webhook-Timestamp", HeaderValue::from_static("1700000000"));
    map.insert("Webhook-Signature", HeaderValue::from_static(SIGNATURE));
    assert!(verifier()
        .verify_at(BODY, WebhookHeaders::from_headers(&map).unwrap(), NOW)
        .is_ok());
    for name in ["webhook-id", "webhook-timestamp", "webhook-signature"] {
        let mut missing = map.clone();
        missing.remove(name);
        assert!(matches!(
            WebhookHeaders::from_headers(&missing),
            Err(WebhookError::MissingHeader(_))
        ));
        let mut duplicate = map.clone();
        duplicate.append(name, HeaderValue::from_static("duplicate"));
        assert!(matches!(
            WebhookHeaders::from_headers(&duplicate),
            Err(WebhookError::InvalidHeader(_))
        ));
    }
    map.insert("webhook-id", HeaderValue::from_bytes(&[255]).unwrap());
    assert!(matches!(
        WebhookHeaders::from_headers(&map),
        Err(WebhookError::InvalidHeader("webhook-id"))
    ));
}

#[test]
fn convenience_unwrap_never_deserializes_an_unverified_payload() {
    static PARSES: AtomicUsize = AtomicUsize::new(0);
    #[derive(Debug)]
    struct Probe;
    impl<'de> Deserialize<'de> for Probe {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            PARSES.fetch_add(1, Ordering::SeqCst);
            let _ = Value::deserialize(deserializer)?;
            Ok(Self)
        }
    }
    let verifier = verifier();
    let error = verifier
        .unwrap_at::<Probe>(b"untrusted bytes", headers(SIGNATURE), NOW)
        .unwrap_err();
    assert!(matches!(error, WebhookError::InvalidSignature));
    assert_eq!(PARSES.load(Ordering::SeqCst), 0);
    verifier
        .unwrap_at::<Probe>(BODY, headers(SIGNATURE), NOW)
        .unwrap();
    assert_eq!(PARSES.load(Ordering::SeqCst), 1);
}
