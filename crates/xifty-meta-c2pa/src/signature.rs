//! COSE_Sign1 (RFC 9052/9053) parse-only decoder.
//!
//! C2PA signature boxes carry a CBOR tag-18 value wrapping a 4-element array:
//!
//! ```text
//! [protected_bstr, unprotected_map, payload_or_nil, signature_bstr]
//! ```
//!
//! The `protected_bstr` is itself a CBOR-encoded map; we extract label `1`
//! (`alg`) per RFC 9053. Label `33` (`x5chain`) may appear in either header;
//! when present we lift the leaf certificate's Subject CN via a minimal DER
//! walker. **No cryptographic verification is performed at this level.**

use ciborium::value::Value;

use crate::cbor;

#[derive(Debug, Default, Clone)]
pub struct SignatureInfo {
    pub alg: Option<String>,
    pub issuer: Option<String>,
}

pub fn decode(bytes: &[u8]) -> SignatureInfo {
    let Some(value) = cbor::from_slice(bytes) else {
        return SignatureInfo::default();
    };
    let inner = match value {
        Value::Tag(18, inner) => *inner,
        other => other,
    };
    let Value::Array(items) = inner else {
        return SignatureInfo::default();
    };
    if items.len() != 4 {
        return SignatureInfo::default();
    }
    let protected_bytes = match &items[0] {
        Value::Bytes(b) => b.clone(),
        _ => return SignatureInfo::default(),
    };
    let unprotected = items[1].clone();

    let mut info = SignatureInfo::default();

    if let Some(protected_map) = cbor::from_slice(&protected_bytes) {
        if let Some(alg) = lookup_alg(&protected_map) {
            info.alg = Some(alg_label(alg));
        }
        if info.issuer.is_none() {
            info.issuer = lookup_x5chain_cn(&protected_map);
        }
    }
    if info.issuer.is_none() {
        info.issuer = lookup_x5chain_cn(&unprotected);
    }
    info
}

fn lookup_alg(value: &Value) -> Option<i64> {
    let Value::Map(pairs) = value else {
        return None;
    };
    for (k, v) in pairs {
        if integer_eq(k, 1) {
            if let Value::Integer(i) = v {
                return i64::try_from(*i).ok();
            }
        }
    }
    None
}

/// COSE alg label (RFC 9053) -> short string label.
fn alg_label(label: i64) -> String {
    match label {
        -7 => "es256".into(),
        -8 => "eddsa".into(),
        -35 => "es384".into(),
        -36 => "es512".into(),
        -37 => "ps256".into(),
        -38 => "ps384".into(),
        -39 => "ps512".into(),
        -257 => "rs256".into(),
        -258 => "rs384".into(),
        -259 => "rs512".into(),
        other => format!("cose_alg_{other}"),
    }
}

fn integer_eq(value: &Value, target: i64) -> bool {
    match value {
        Value::Integer(i) => i64::try_from(*i).map(|v| v == target).unwrap_or(false),
        _ => false,
    }
}

/// Find CBOR map label 33 (`x5chain`) and return the leaf certificate Subject CN.
fn lookup_x5chain_cn(value: &Value) -> Option<String> {
    let Value::Map(pairs) = value else {
        return None;
    };
    for (k, v) in pairs {
        if integer_eq(k, 33) {
            return cn_from_x5chain(v);
        }
    }
    None
}

fn cn_from_x5chain(value: &Value) -> Option<String> {
    // x5chain is either a single bstr (one cert) or an array of bstrs.
    let leaf = match value {
        Value::Bytes(b) => b.clone(),
        Value::Array(items) => match items.first()? {
            Value::Bytes(b) => b.clone(),
            _ => return None,
        },
        _ => return None,
    };
    cn_from_certificate(&leaf)
}

/// Minimal DER walker. Locates the Subject SEQUENCE inside a TBSCertificate
/// and returns the first AttributeTypeAndValue whose OID is CN (2.5.4.3).
pub fn cn_from_certificate(der: &[u8]) -> Option<String> {
    // Certificate ::= SEQUENCE { tbsCertificate, signatureAlgorithm, signatureValue }
    let (cert_inner, _) = der_seq(der)?;
    // tbsCertificate ::= SEQUENCE { version [0] EXPLICIT? , serialNumber INTEGER,
    //                               signature AlgorithmIdentifier, issuer Name,
    //                               validity, subject Name, ... }
    let (tbs_inner, _) = der_seq(cert_inner)?;
    let mut cur = tbs_inner;
    // Optional [0] version.
    if let Some(rest) = consume_explicit_tag(cur, 0) {
        cur = rest;
    }
    // serialNumber INTEGER
    cur = skip_tlv(cur)?;
    // signature AlgorithmIdentifier (SEQUENCE)
    cur = skip_tlv(cur)?;
    // issuer Name (SEQUENCE)
    cur = skip_tlv(cur)?;
    // validity (SEQUENCE)
    cur = skip_tlv(cur)?;
    // subject Name (SEQUENCE)
    let (subject_inner, _) = der_seq(cur)?;
    cn_from_name(subject_inner)
}

fn consume_explicit_tag<'a>(input: &'a [u8], tag_number: u8) -> Option<&'a [u8]> {
    if input.is_empty() {
        return None;
    }
    let tag = input[0];
    // Context-specific [n] EXPLICIT: 0xA0 | tag_number.
    if tag != 0xA0 | tag_number {
        return None;
    }
    let (_, rest) = read_tlv(input)?;
    Some(rest)
}

fn cn_from_name(name_inner: &[u8]) -> Option<String> {
    // Name ::= SEQUENCE OF RelativeDistinguishedName
    // RDN ::= SET OF AttributeTypeAndValue
    // ATV ::= SEQUENCE { type OID, value ANY }
    let mut cursor = name_inner;
    while !cursor.is_empty() {
        let (rdn_inner, rest) = match read_set(cursor) {
            Some(v) => v,
            None => break,
        };
        let mut atv_cur = rdn_inner;
        while !atv_cur.is_empty() {
            let (atv_inner, atv_rest) = match der_seq(atv_cur) {
                Some(v) => v,
                None => break,
            };
            // OID
            let (oid, after_oid) = read_oid(atv_inner)?;
            if oid == [2, 5, 4, 3] {
                if let Some(s) = read_directory_string(after_oid) {
                    return Some(s);
                }
            }
            atv_cur = atv_rest;
        }
        cursor = rest;
    }
    None
}

fn der_seq(input: &[u8]) -> Option<(&[u8], &[u8])> {
    if input.is_empty() || input[0] != 0x30 {
        return None;
    }
    read_tlv(input)
}

fn read_set(input: &[u8]) -> Option<(&[u8], &[u8])> {
    if input.is_empty() || input[0] != 0x31 {
        return None;
    }
    read_tlv(input)
}

/// Read a TLV, returning (content, remaining-after-tlv).
fn read_tlv(input: &[u8]) -> Option<(&[u8], &[u8])> {
    if input.len() < 2 {
        return None;
    }
    let mut p = 1usize;
    let len_byte = input[p];
    p += 1;
    let length = if len_byte & 0x80 == 0 {
        len_byte as usize
    } else {
        let n = (len_byte & 0x7F) as usize;
        if n == 0 || n > 4 || p + n > input.len() {
            return None;
        }
        let mut l = 0usize;
        for _ in 0..n {
            l = (l << 8) | input[p] as usize;
            p += 1;
        }
        l
    };
    if p + length > input.len() {
        return None;
    }
    Some((&input[p..p + length], &input[p + length..]))
}

fn skip_tlv(input: &[u8]) -> Option<&[u8]> {
    let (_, rest) = read_tlv(input)?;
    Some(rest)
}

fn read_oid(input: &[u8]) -> Option<(Vec<u32>, &[u8])> {
    if input.is_empty() || input[0] != 0x06 {
        return None;
    }
    let (content, rest) = read_tlv(input)?;
    if content.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    let first = content[0];
    out.push((first / 40) as u32);
    out.push((first % 40) as u32);
    let mut value = 0u32;
    for &b in &content[1..] {
        value = (value << 7) | (b & 0x7F) as u32;
        if b & 0x80 == 0 {
            out.push(value);
            value = 0;
        }
    }
    Some((out, rest))
}

fn read_directory_string(input: &[u8]) -> Option<String> {
    if input.is_empty() {
        return None;
    }
    let tag = input[0];
    let (content, _) = read_tlv(input)?;
    match tag {
        0x0C | 0x13 | 0x16 => std::str::from_utf8(content).ok().map(|s| s.to_string()),
        0x1E => {
            // BMPString — UTF-16BE.
            let mut codes = Vec::with_capacity(content.len() / 2);
            for chunk in content.chunks_exact(2) {
                codes.push(u16::from_be_bytes([chunk[0], chunk[1]]));
            }
            String::from_utf16(&codes).ok()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ciborium::value::Integer;

    fn encode(value: &Value) -> Vec<u8> {
        let mut out = Vec::new();
        ciborium::ser::into_writer(value, &mut out).unwrap();
        out
    }

    #[test]
    fn alg_table_maps_known_labels() {
        for (label, expected) in [
            (-7, "es256"),
            (-8, "eddsa"),
            (-35, "es384"),
            (-36, "es512"),
            (-37, "ps256"),
            (-257, "rs256"),
        ] {
            assert_eq!(alg_label(label), expected);
        }
    }

    #[test]
    fn decodes_minimal_cose_sign1_eddsa() {
        // protected map: {1: -8}
        let protected_map = Value::Map(vec![(
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(-8)),
        )]);
        let mut protected_bytes = Vec::new();
        ciborium::ser::into_writer(&protected_map, &mut protected_bytes).unwrap();

        let cose = Value::Tag(
            18,
            Box::new(Value::Array(vec![
                Value::Bytes(protected_bytes),
                Value::Map(Vec::new()),
                Value::Bytes(Vec::new()),
                Value::Bytes(vec![0u8; 64]),
            ])),
        );
        let bytes = encode(&cose);
        let info = decode(&bytes);
        assert_eq!(info.alg.as_deref(), Some("eddsa"));
        assert!(info.issuer.is_none());
    }

    /// Hand-craft a minimal X.509 with Subject CN="Example Issuer", and
    /// verify the walker extracts it. Build TBSCertificate with skeletal
    /// fields: version[0]=v3, serial=1, alg=null, issuer=empty, validity=empty,
    /// subject=CN, then wrap.
    #[test]
    fn cn_walker_extracts_utf8string_cn() {
        // OID 2.5.4.3 -> bytes 0x55, 0x04, 0x03
        let cn_value = b"Example Issuer";
        let mut atv = Vec::new();
        // OID
        atv.push(0x06);
        atv.push(3);
        atv.extend_from_slice(&[0x55, 0x04, 0x03]);
        // UTF8String
        atv.push(0x0C);
        atv.push(cn_value.len() as u8);
        atv.extend_from_slice(cn_value);
        let mut atv_seq = Vec::new();
        atv_seq.push(0x30);
        atv_seq.push(atv.len() as u8);
        atv_seq.extend_from_slice(&atv);
        let mut rdn = Vec::new();
        rdn.push(0x31);
        rdn.push(atv_seq.len() as u8);
        rdn.extend_from_slice(&atv_seq);
        let mut subject_seq = Vec::new();
        subject_seq.push(0x30);
        subject_seq.push(rdn.len() as u8);
        subject_seq.extend_from_slice(&rdn);

        let mut tbs = Vec::new();
        // [0] EXPLICIT version=2 (v3)
        tbs.extend_from_slice(&[0xA0, 0x03, 0x02, 0x01, 0x02]);
        // serialNumber INTEGER 1
        tbs.extend_from_slice(&[0x02, 0x01, 0x01]);
        // signature AlgorithmIdentifier ::= SEQUENCE { OID, NULL }
        tbs.extend_from_slice(&[0x30, 0x05, 0x06, 0x03, 0x55, 0x04, 0x03]);
        // issuer Name (empty SEQUENCE OF RDN)
        tbs.extend_from_slice(&[0x30, 0x00]);
        // validity (empty SEQUENCE)
        tbs.extend_from_slice(&[0x30, 0x00]);
        // subject Name
        tbs.extend_from_slice(&subject_seq);

        let mut tbs_seq = Vec::new();
        tbs_seq.push(0x30);
        tbs_seq.push(tbs.len() as u8);
        tbs_seq.extend_from_slice(&tbs);

        // Wrap into Certificate ::= SEQUENCE { tbs, alg, sig }
        let mut cert = Vec::new();
        cert.extend_from_slice(&tbs_seq);
        cert.extend_from_slice(&[0x30, 0x05, 0x06, 0x03, 0x55, 0x04, 0x03]);
        cert.extend_from_slice(&[0x03, 0x02, 0x00, 0x00]);

        let mut cert_seq = Vec::new();
        cert_seq.push(0x30);
        // length needs 2-byte form if > 127
        if cert.len() < 128 {
            cert_seq.push(cert.len() as u8);
        } else {
            cert_seq.push(0x82);
            cert_seq.extend_from_slice(&(cert.len() as u16).to_be_bytes());
        }
        cert_seq.extend_from_slice(&cert);

        let cn = cn_from_certificate(&cert_seq).unwrap();
        assert_eq!(cn, "Example Issuer");
    }
}
