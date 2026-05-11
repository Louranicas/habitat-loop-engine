#![forbid(unsafe_code)]

// End-to-end stack cross-reference: terminal implementation node for
// M005_RECEIPT_HASH.md / L01_FOUNDATION.md / C01_EVIDENCE_INTEGRITY (cluster).
// Spec: ai_specs/modules/c01-evidence-integrity/M005_RECEIPT_HASH.md.
//
// M0 IMPLEMENTATION (2026-05-11): `from_fields` uses real SHA-256 via the
// `sha2` crate (workspace's first external dep). Verified against documented
// test vectors. The canonical-bytes serialization is the contract surface;
// any change requires the spec sheet + receipt_sha_verifier (M008) to be
// re-aligned in lockstep.

use sha2::{Digest, Sha256};
use std::fmt;
use substrate_types::HleError;

/// Algorithm tag for the receipt hashing primitive.
///
/// Only `Sha256` is supported. The tag is carried in log output and future
/// wire protocols to allow algorithm agility without string magic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HashAlgorithm {
    /// SHA-256 producing a 32-byte digest. The only supported variant.
    Sha256,
}

impl HashAlgorithm {
    /// Returns the wire-format algorithm identifier string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
        }
    }

    /// Returns the byte length of the digest produced by this algorithm.
    #[must_use]
    pub const fn digest_len(self) -> usize {
        match self {
            Self::Sha256 => 32,
        }
    }
}

impl fmt::Display for HashAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical receipt hashing; source of all proof identity in C01.
///
/// `ReceiptHash` is the single receipt identifier accepted by the claim store
/// (M006), the receipts store (M007), and the SHA verifier (M008). It
/// corresponds to the `manifest_sha256` / `framework_sha256` split-hash
/// anchors in `HARNESS_CONTRACT.md` and satisfies the pattern
/// `^[0-9a-f]{64}$` required by `schemas/receipt.schema.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReceiptHash([u8; 32]);

impl ReceiptHash {
    /// Construct directly from a 32-byte array (raw deserialization path).
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Compute a `ReceiptHash` from the canonical receipt fields using SHA-256.
    ///
    /// The canonical-bytes serialization (`workflow \x00 step_id \x00 verdict
    /// \x00 manifest_sha256 \x00 framework_sha256`) is fed to SHA-256 to produce
    /// the 32-byte digest. M008 `receipt_sha_verifier` re-computes via this
    /// same function for adversarial recompute — any change to the
    /// serialization contract here MUST land alongside spec + verifier updates.
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2000] HashInput`) when `fields.workflow` is empty,
    /// or when `canonical_bytes` cannot be constructed.
    pub fn from_fields(fields: &ReceiptHashFields) -> Result<Self, HleError> {
        let raw = fields.canonical_bytes()?;
        let digest: [u8; 32] = Sha256::digest(&raw).into();
        Ok(Self(digest))
    }

    /// Compute a `ReceiptHash` directly from an arbitrary byte slice using SHA-256.
    ///
    /// Useful for hashing pre-serialized canonical content (e.g., a JSON-line
    /// receipt body) outside the `ReceiptHashFields` shape. The primary
    /// authority path remains `from_fields`.
    #[must_use]
    pub fn from_bytes_sha256(bytes: &[u8]) -> Self {
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        Self(digest)
    }

    /// Sentinel zero-hash. Use only in tests and negative controls.
    ///
    /// A zeroed `ReceiptHash` must never appear in a production receipt
    /// graph; `ReceiptsStore::append` rejects it with `[E2020] AppendConflict`.
    #[must_use]
    pub const fn zeroed() -> Self {
        Self([0u8; 32])
    }

    /// Returns a reference to the raw 32-byte digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns the 64-character lowercase hex representation.
    ///
    /// Output satisfies the JSON schema pattern `^[0-9a-f]{64}$`.
    #[must_use]
    pub fn to_hex(&self) -> String {
        use std::fmt::Write as _;
        self.0.iter().fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
    }

    /// Parse a 64-character lowercase hex string into a `ReceiptHash`.
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2000] HashInput`) when the input is not exactly
    /// 64 lowercase hex characters.
    pub fn from_hex(hex: &str) -> Result<Self, HleError> {
        if hex.len() != 64 {
            return Err(HleError::new(format!(
                "[E2000] HashInput: expected 64 hex chars, got {}",
                hex.len()
            )));
        }
        let mut bytes = [0u8; 32];
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            let hi = hex_nibble(chunk[0]).map_err(|()| {
                HleError::new(format!(
                    "[E2000] HashInput: invalid hex char at position {}",
                    i * 2
                ))
            })?;
            let lo = hex_nibble(chunk[1]).map_err(|()| {
                HleError::new(format!(
                    "[E2000] HashInput: invalid hex char at position {}",
                    i * 2 + 1
                ))
            })?;
            bytes[i] = (hi << 4) | lo;
        }
        Ok(Self(bytes))
    }
}

fn hex_nibble(byte: u8) -> Result<u8, ()> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(()),
    }
}

impl fmt::Display for ReceiptHash {
    /// Emits the first 16 hex chars followed by `…` for log brevity.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = self.to_hex();
        write!(f, "{}…", &hex[..16])
    }
}

impl AsRef<[u8]> for ReceiptHash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// The exact fields fed to the hash function for canonical receipt hashing.
///
/// `canonical_bytes` produces the deterministic byte sequence ingested by
/// `ReceiptHash::from_fields`. Fields are joined with null-byte (`\x00`)
/// separators so that `workflow="ab"`, `step_id="c"` produces a different
/// digest than `workflow="a"`, `step_id="bc"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptHashFields {
    /// Workflow identifier. Must be non-empty.
    pub workflow: String,
    /// Step identifier within the workflow.
    pub step_id: String,
    /// Verifier verdict string (e.g. `"PASS"`, `"FAIL"`, `"AWAITING_HUMAN"`).
    pub verdict: String,
    /// `manifest_sha256` anchor — scaffold manifest hash per `HARNESS_CONTRACT.md`.
    pub manifest_sha256: String,
    /// `framework_sha256` anchor — source/framework provenance hash per `HARNESS_CONTRACT.md`.
    pub framework_sha256: String,
}

impl ReceiptHashFields {
    /// Construct and validate receipt hash fields.
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2000] HashInput`) when `workflow` is empty.
    pub fn new(
        workflow: impl Into<String>,
        step_id: impl Into<String>,
        verdict: impl Into<String>,
        manifest_sha256: impl Into<String>,
        framework_sha256: impl Into<String>,
    ) -> Result<Self, HleError> {
        let workflow = workflow.into();
        if workflow.trim().is_empty() {
            return Err(HleError::new(
                "[E2000] HashInput: workflow name must be non-empty",
            ));
        }
        Ok(Self {
            workflow,
            step_id: step_id.into(),
            verdict: verdict.into(),
            manifest_sha256: manifest_sha256.into(),
            framework_sha256: framework_sha256.into(),
        })
    }

    /// Deterministic serialization fed to the hash function.
    ///
    /// Fields are concatenated with `\x00` null-byte separators. The
    /// separator choice prevents hash collisions from field boundary confusion.
    ///
    /// # Errors
    ///
    /// Returns `Err` when the workflow field is empty (defensive re-check).
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, HleError> {
        if self.workflow.trim().is_empty() {
            return Err(HleError::new(
                "[E2000] HashInput: workflow name must be non-empty for canonical serialization",
            ));
        }
        let mut out = Vec::new();
        out.extend_from_slice(self.workflow.as_bytes());
        out.push(0u8);
        out.extend_from_slice(self.step_id.as_bytes());
        out.push(0u8);
        out.extend_from_slice(self.verdict.as_bytes());
        out.push(0u8);
        out.extend_from_slice(self.manifest_sha256.as_bytes());
        out.push(0u8);
        out.extend_from_slice(self.framework_sha256.as_bytes());
        Ok(out)
    }
}

impl fmt::Display for ReceiptHashFields {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fields({}/{})", self.workflow, self.step_id)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    // ── helpers ────────────────────────────────────────────────────────────────

    fn fields(wf: &str, step: &str) -> ReceiptHashFields {
        ReceiptHashFields::new(wf, step, "PASS", "", "").expect("fields must be valid")
    }

    fn full_fields(
        wf: &str,
        step: &str,
        verdict: &str,
        manifest: &str,
        framework: &str,
    ) -> ReceiptHashFields {
        ReceiptHashFields::new(wf, step, verdict, manifest, framework)
            .expect("fields must be valid")
    }

    // ── zeroed ────────────────────────────────────────────────────────────────

    #[test]
    fn zeroed_is_all_zero_bytes() {
        assert_eq!(ReceiptHash::zeroed().as_bytes(), &[0u8; 32]);
    }

    #[test]
    fn zeroed_to_hex_is_64_zeros() {
        assert_eq!(ReceiptHash::zeroed().to_hex(), "0".repeat(64));
    }

    #[test]
    fn zeroed_from_bytes_roundtrip() {
        let z = ReceiptHash::zeroed();
        let rt = ReceiptHash::from_bytes(*z.as_bytes());
        assert_eq!(z, rt);
    }

    // ── from_bytes / as_bytes round-trip ──────────────────────────────────────

    #[test]
    fn from_bytes_as_bytes_roundtrip_arbitrary() {
        let arr: [u8; 32] = core::array::from_fn(|i| i as u8);
        let h = ReceiptHash::from_bytes(arr);
        assert_eq!(h.as_bytes(), &arr);
    }

    #[test]
    fn from_bytes_as_bytes_roundtrip_all_ff() {
        let h = ReceiptHash::from_bytes([0xFFu8; 32]);
        assert_eq!(h.as_bytes(), &[0xFFu8; 32]);
    }

    #[test]
    fn from_bytes_as_ref_matches_as_bytes() {
        let arr: [u8; 32] = core::array::from_fn(|i| (i * 3) as u8);
        let h = ReceiptHash::from_bytes(arr);
        assert_eq!(h.as_ref(), h.as_bytes() as &[u8]);
    }

    // ── to_hex / from_hex round-trips ─────────────────────────────────────────

    #[test]
    fn from_hex_round_trips_with_to_hex() {
        let original = ReceiptHash::from_bytes([
            0x3a, 0x7f, 0x9c, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a,
            0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
            0x19, 0x1a, 0x1b, 0x1c,
        ]);
        let hex = original.to_hex();
        assert_eq!(hex.len(), 64);
        let roundtripped = ReceiptHash::from_hex(&hex).expect("round-trip must succeed");
        assert_eq!(original, roundtripped);
    }

    #[test]
    fn from_hex_rejects_empty_string() {
        assert!(ReceiptHash::from_hex("").is_err());
    }

    #[test]
    fn from_hex_rejects_short_input() {
        assert!(ReceiptHash::from_hex("abc").is_err());
    }

    #[test]
    fn from_hex_rejects_63_chars() {
        let s: String = "a".repeat(63);
        assert!(ReceiptHash::from_hex(&s).is_err());
    }

    #[test]
    fn from_hex_rejects_65_chars() {
        let s: String = "a".repeat(65);
        assert!(ReceiptHash::from_hex(&s).is_err());
    }

    #[test]
    fn from_hex_rejects_invalid_chars() {
        let bad: String = "g".repeat(64);
        assert!(ReceiptHash::from_hex(&bad).is_err());
    }

    #[test]
    fn from_hex_rejects_uppercase_chars() {
        // uppercase A–F are not valid per the `^[0-9a-f]{64}$` schema pattern
        let upper: String = "A".repeat(64);
        assert!(ReceiptHash::from_hex(&upper).is_err());
    }

    #[test]
    fn from_hex_error_message_contains_e2000() {
        let err = ReceiptHash::from_hex("bad").unwrap_err();
        assert!(err.to_string().contains("E2000"), "got: {err}");
    }

    #[test]
    fn from_hex_round_trips_all_zeros() {
        let hex = "0".repeat(64);
        let h = ReceiptHash::from_hex(&hex).expect("must parse");
        assert_eq!(h.to_hex(), hex);
    }

    #[test]
    fn from_hex_round_trips_all_ff() {
        let hex = "f".repeat(64);
        let h = ReceiptHash::from_hex(&hex).expect("must parse");
        assert_eq!(h.to_hex(), hex);
    }

    #[test]
    fn to_hex_is_always_lowercase() {
        let h = ReceiptHash::from_bytes([0xABu8; 32]);
        let hex = h.to_hex();
        assert!(hex.chars().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn to_hex_len_is_always_64() {
        for byte in [0x00u8, 0x0F, 0x10, 0xFF] {
            let h = ReceiptHash::from_bytes([byte; 32]);
            assert_eq!(h.to_hex().len(), 64);
        }
    }

    // ── Display (abbreviated) ─────────────────────────────────────────────────

    #[test]
    fn display_shows_abbreviated_hex() {
        let h = ReceiptHash::zeroed();
        let s = format!("{h}");
        assert!(s.ends_with('…'));
        // 16 hex chars + 1 Unicode ellipsis char = 17 chars; byte len is 19 (ellipsis = 3 bytes).
        assert_eq!(s.chars().count(), 17);
    }

    #[test]
    fn display_first_16_chars_match_to_hex_prefix() {
        let h = ReceiptHash::from_bytes_sha256(b"display test");
        let full = h.to_hex();
        let display = format!("{h}");
        let prefix: String = display.chars().take(16).collect();
        assert_eq!(prefix, &full[..16]);
    }

    #[test]
    fn display_ends_with_ellipsis_for_all_ones() {
        let h = ReceiptHash::from_bytes([0xFFu8; 32]);
        let s = format!("{h}");
        assert!(s.ends_with('…'));
    }

    // ── HashAlgorithm ─────────────────────────────────────────────────────────

    #[test]
    fn hash_algorithm_sha256_str_is_stable() {
        assert_eq!(HashAlgorithm::Sha256.as_str(), "sha256");
    }

    #[test]
    fn hash_algorithm_sha256_digest_len_is_32() {
        assert_eq!(HashAlgorithm::Sha256.digest_len(), 32);
    }

    #[test]
    fn hash_algorithm_display_matches_as_str() {
        assert_eq!(
            HashAlgorithm::Sha256.to_string(),
            HashAlgorithm::Sha256.as_str()
        );
    }

    #[test]
    fn hash_algorithm_eq_reflexive() {
        assert_eq!(HashAlgorithm::Sha256, HashAlgorithm::Sha256);
    }

    #[test]
    fn hash_algorithm_hash_usable_in_hashset() {
        let mut s = HashSet::new();
        s.insert(HashAlgorithm::Sha256);
        assert!(s.contains(&HashAlgorithm::Sha256));
    }

    // ── ReceiptHashFields construction ────────────────────────────────────────

    #[test]
    fn from_fields_rejects_empty_workflow() {
        let result = ReceiptHashFields::new("", "s1", "PASS", "", "");
        assert!(result.is_err());
    }

    #[test]
    fn receipt_hash_fields_new_rejects_blank_workflow() {
        assert!(ReceiptHashFields::new("   ", "s1", "PASS", "", "").is_err());
    }

    #[test]
    fn fields_new_error_contains_e2000() {
        let err = ReceiptHashFields::new("", "s1", "PASS", "", "").unwrap_err();
        assert!(err.to_string().contains("E2000"), "got: {err}");
    }

    #[test]
    fn fields_new_accepts_tab_only_step_id() {
        // step_id is not validated for emptiness — only workflow is
        let f = ReceiptHashFields::new("demo", "\t", "PASS", "", "");
        assert!(f.is_ok());
    }

    #[test]
    fn fields_display_contains_workflow_and_step() {
        let f = fields("wf", "s99");
        let s = format!("{f}");
        assert!(s.contains("wf"));
        assert!(s.contains("s99"));
    }

    // ── canonical_bytes ───────────────────────────────────────────────────────

    #[test]
    fn canonical_bytes_differ_for_different_field_boundaries() {
        let f1 = ReceiptHashFields::new("ab", "c", "PASS", "", "").expect("fields must be valid");
        let f2 = ReceiptHashFields::new("a", "bc", "PASS", "", "").expect("fields must be valid");
        assert_ne!(
            f1.canonical_bytes().expect("bytes must be ok"),
            f2.canonical_bytes().expect("bytes must be ok"),
        );
    }

    #[test]
    fn canonical_bytes_contain_null_separators() {
        let f = fields("wf", "s1");
        let bytes = f.canonical_bytes().expect("must be ok");
        assert!(bytes.contains(&0u8));
    }

    #[test]
    fn canonical_bytes_are_deterministic() {
        let f = full_fields("demo", "s1", "PASS", "ma", "fr");
        assert_eq!(
            f.canonical_bytes().expect("ok"),
            f.canonical_bytes().expect("ok")
        );
    }

    #[test]
    fn canonical_bytes_differ_when_verdict_changes() {
        let f1 = full_fields("wf", "s1", "PASS", "", "");
        let f2 = full_fields("wf", "s1", "FAIL", "", "");
        assert_ne!(
            f1.canonical_bytes().expect("ok"),
            f2.canonical_bytes().expect("ok")
        );
    }

    #[test]
    fn canonical_bytes_differ_when_manifest_changes() {
        let f1 = full_fields("wf", "s1", "PASS", "aaa", "");
        let f2 = full_fields("wf", "s1", "PASS", "bbb", "");
        assert_ne!(
            f1.canonical_bytes().expect("ok"),
            f2.canonical_bytes().expect("ok")
        );
    }

    #[test]
    fn canonical_bytes_differ_when_framework_changes() {
        let f1 = full_fields("wf", "s1", "PASS", "", "aaa");
        let f2 = full_fields("wf", "s1", "PASS", "", "bbb");
        assert_ne!(
            f1.canonical_bytes().expect("ok"),
            f2.canonical_bytes().expect("ok")
        );
    }

    #[test]
    fn canonical_bytes_rejects_empty_workflow() {
        let f = ReceiptHashFields {
            workflow: String::new(),
            step_id: String::from("s1"),
            verdict: String::from("PASS"),
            manifest_sha256: String::new(),
            framework_sha256: String::new(),
        };
        assert!(f.canonical_bytes().is_err());
    }

    // ── from_fields ───────────────────────────────────────────────────────────

    #[test]
    fn from_fields_produces_deterministic_hash() {
        let f = full_fields("demo", "s1", "PASS", "msha", "fsha");
        let h1 = ReceiptHash::from_fields(&f).expect("hash must succeed");
        let h2 = ReceiptHash::from_fields(&f).expect("hash must succeed");
        assert_eq!(h1, h2);
    }

    #[test]
    fn from_fields_different_workflows_produce_different_hashes() {
        let f1 = fields("wf-a", "s1");
        let f2 = fields("wf-b", "s1");
        assert_ne!(
            ReceiptHash::from_fields(&f1).expect("ok"),
            ReceiptHash::from_fields(&f2).expect("ok")
        );
    }

    #[test]
    fn from_fields_different_steps_produce_different_hashes() {
        let f1 = fields("demo", "step-1");
        let f2 = fields("demo", "step-2");
        assert_ne!(
            ReceiptHash::from_fields(&f1).expect("ok"),
            ReceiptHash::from_fields(&f2).expect("ok")
        );
    }

    #[test]
    fn from_fields_different_verdicts_produce_different_hashes() {
        let f1 = full_fields("demo", "s1", "PASS", "", "");
        let f2 = full_fields("demo", "s1", "FAIL", "", "");
        assert_ne!(
            ReceiptHash::from_fields(&f1).expect("ok"),
            ReceiptHash::from_fields(&f2).expect("ok")
        );
    }

    // ── SHA-256 known vectors ─────────────────────────────────────────────────

    /// NIST FIPS 180-4 vector: SHA-256("") = e3b0c44...
    #[test]
    fn sha256_known_vector_empty_input() {
        let h = ReceiptHash::from_bytes_sha256(b"");
        assert_eq!(
            h.to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    /// NIST FIPS 180-4 vector: SHA-256("abc") = ba7816bf...
    #[test]
    fn sha256_known_vector_abc() {
        let h = ReceiptHash::from_bytes_sha256(b"abc");
        assert_eq!(
            h.to_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// NIST FIPS 180-4 — two-block boundary (448-bit message): SHA-256("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")
    #[test]
    fn sha256_known_vector_two_block_boundary() {
        let h = ReceiptHash::from_bytes_sha256(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
        );
        assert_eq!(
            h.to_hex(),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    /// Long input (> 64 bytes) exercises multi-block path.
    #[test]
    fn sha256_long_input_is_deterministic() {
        let input: Vec<u8> = (0u8..=127).collect();
        let h1 = ReceiptHash::from_bytes_sha256(&input);
        let h2 = ReceiptHash::from_bytes_sha256(&input);
        assert_eq!(h1, h2);
    }

    /// One-byte inputs produce different digests for different bytes.
    #[test]
    fn sha256_single_byte_inputs_produce_distinct_hashes() {
        let h0 = ReceiptHash::from_bytes_sha256(b"\x00");
        let h1 = ReceiptHash::from_bytes_sha256(b"\x01");
        assert_ne!(h0, h1);
    }

    #[test]
    fn sha256_from_fields_matches_independent_recompute() {
        let f = full_fields("demo", "s1", "PASS", "ma", "fr");
        let from_fields = ReceiptHash::from_fields(&f).expect("hash must succeed");
        let from_bytes = ReceiptHash::from_bytes_sha256(&f.canonical_bytes().expect("bytes ok"));
        assert_eq!(from_fields, from_bytes);
    }

    #[test]
    fn sha256_field_boundary_attack_produces_different_digests() {
        let f1 = fields("ab", "c");
        let f2 = fields("a", "bc");
        let h1 = ReceiptHash::from_fields(&f1).expect("ok");
        let h2 = ReceiptHash::from_fields(&f2).expect("ok");
        assert_ne!(h1, h2);
    }

    #[test]
    fn sha256_hex_matches_schema_pattern() {
        let h = ReceiptHash::from_bytes_sha256(b"any input");
        let hex = h.to_hex();
        assert_eq!(hex.len(), 64);
        assert!(hex
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    /// Tampered single byte in input produces a different hash (tamper-resistance).
    #[test]
    fn sha256_flipping_one_byte_changes_hash() {
        let base = b"receipt-data-for-tamper-test";
        let h_original = ReceiptHash::from_bytes_sha256(base);
        let mut tampered = base.to_vec();
        tampered[0] ^= 0x01;
        let h_tampered = ReceiptHash::from_bytes_sha256(&tampered);
        assert_ne!(h_original, h_tampered);
    }

    // ── Eq / Hash / Ord consistency ───────────────────────────────────────────

    #[test]
    fn eq_is_reflexive() {
        let h = ReceiptHash::from_bytes_sha256(b"test");
        assert_eq!(h, h);
    }

    #[test]
    fn eq_is_symmetric() {
        let h1 = ReceiptHash::from_bytes_sha256(b"sym");
        let h2 = ReceiptHash::from_bytes_sha256(b"sym");
        assert_eq!(h1, h2);
        assert_eq!(h2, h1);
    }

    #[test]
    fn ne_for_different_hashes() {
        let h1 = ReceiptHash::from_bytes_sha256(b"a");
        let h2 = ReceiptHash::from_bytes_sha256(b"b");
        assert_ne!(h1, h2);
    }

    #[test]
    fn usable_as_hashmap_key() {
        let mut map: HashMap<ReceiptHash, &str> = HashMap::new();
        let h = ReceiptHash::from_bytes_sha256(b"key");
        map.insert(h, "value");
        assert_eq!(map.get(&h), Some(&"value"));
    }

    #[test]
    fn two_equal_hashes_have_equal_hashmap_lookup() {
        let mut map: HashMap<ReceiptHash, u32> = HashMap::new();
        let h1 = ReceiptHash::from_bytes_sha256(b"lookup");
        let h2 = ReceiptHash::from_bytes_sha256(b"lookup");
        map.insert(h1, 42);
        assert_eq!(map.get(&h2), Some(&42));
    }

    #[test]
    fn ord_zeroed_less_than_all_ones() {
        let lo = ReceiptHash::zeroed();
        let hi = ReceiptHash::from_bytes([0xFFu8; 32]);
        assert!(lo < hi);
    }

    #[test]
    fn ord_consistent_with_eq() {
        let h = ReceiptHash::from_bytes_sha256(b"ord");
        assert!(!(h < h));
        assert!(!(h > h));
    }

    #[test]
    fn hash_usable_in_hashset_dedup() {
        let mut s = HashSet::new();
        let h = ReceiptHash::from_bytes_sha256(b"dedup");
        s.insert(h);
        s.insert(h); // second insert is a no-op
        assert_eq!(s.len(), 1);
    }

    // ── Debug / Clone / Copy ──────────────────────────────────────────────────

    #[test]
    fn debug_output_is_non_empty() {
        let h = ReceiptHash::zeroed();
        assert!(!format!("{h:?}").is_empty());
    }

    #[test]
    fn clone_produces_equal_value() {
        let h = ReceiptHash::from_bytes_sha256(b"clone");
        assert_eq!(h, h);
    }
}
