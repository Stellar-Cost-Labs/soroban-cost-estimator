//! XDR encoding/decoding roundtrip validation tests.
//!
//! Tests verify that `decode(encode(val)) == val` for all custom XDR structures,
//! ScVal representations, and transaction envelopes.

use stellar_xdr::{ReadXdr, VecM, WriteXdr};

/// Helper to test roundtrip encoding/decoding of any XDR type.
fn test_roundtrip<T>(value: T)
where
    T: WriteXdr + ReadXdr + PartialEq + std::fmt::Debug,
{
    let encoded = value
        .to_xdr(stellar_xdr::Limits::none())
        .expect("encoding should succeed");
    let decoded =
        T::from_xdr(&encoded, stellar_xdr::Limits::none()).expect("decoding should succeed");
    assert_eq!(value, decoded, "roundtrip should preserve value");
}

// ─────────────────────────────────────────────────────────────────────────
// ScVal roundtrip tests
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_scval_bool_roundtrip() {
    test_roundtrip(stellar_xdr::ScVal::Bool(true));
    test_roundtrip(stellar_xdr::ScVal::Bool(false));
}

#[test]
fn test_scval_i32_roundtrip() {
    test_roundtrip(stellar_xdr::ScVal::I32(42));
    test_roundtrip(stellar_xdr::ScVal::I32(-42));
    test_roundtrip(stellar_xdr::ScVal::I32(i32::MAX));
    test_roundtrip(stellar_xdr::ScVal::I32(i32::MIN));
}

#[test]
fn test_scval_i64_roundtrip() {
    test_roundtrip(stellar_xdr::ScVal::I64(42));
    test_roundtrip(stellar_xdr::ScVal::I64(-42));
    test_roundtrip(stellar_xdr::ScVal::I64(i64::MAX));
    test_roundtrip(stellar_xdr::ScVal::I64(i64::MIN));
}

#[test]
fn test_scval_u32_roundtrip() {
    test_roundtrip(stellar_xdr::ScVal::U32(42));
    test_roundtrip(stellar_xdr::ScVal::U32(u32::MAX));
}

#[test]
fn test_scval_u64_roundtrip() {
    test_roundtrip(stellar_xdr::ScVal::U64(42));
    test_roundtrip(stellar_xdr::ScVal::U64(u64::MAX));
}

#[test]
fn test_scval_symbol_roundtrip() {
    let s: stellar_xdr::ScSymbol = stellar_xdr::ScSymbol::try_from(b"test".to_vec()).unwrap();
    test_roundtrip(stellar_xdr::ScVal::Symbol(s));

    let s: stellar_xdr::ScSymbol = stellar_xdr::ScSymbol::try_from(b"a".repeat(32)).unwrap();
    test_roundtrip(stellar_xdr::ScVal::Symbol(s));
}

#[test]
fn test_scval_string_roundtrip() {
    let s: stellar_xdr::ScString = stellar_xdr::StringM::try_from(b"hello".to_vec())
        .map(stellar_xdr::ScString::from)
        .unwrap();
    test_roundtrip(stellar_xdr::ScVal::String(s));

    let s: stellar_xdr::ScString = stellar_xdr::StringM::try_from(b"".to_vec())
        .map(stellar_xdr::ScString::from)
        .unwrap();
    test_roundtrip(stellar_xdr::ScVal::String(s));
}

#[test]
fn test_scval_vec_roundtrip() {
    let vec: VecM<stellar_xdr::ScVal> = vec![
        stellar_xdr::ScVal::I32(1),
        stellar_xdr::ScVal::I32(2),
        stellar_xdr::ScVal::I32(3),
    ]
    .try_into()
    .unwrap();
    test_roundtrip(stellar_xdr::ScVal::Vec(Some(stellar_xdr::ScVec(vec))));

    let vec: VecM<stellar_xdr::ScVal> = vec![].try_into().unwrap();
    test_roundtrip(stellar_xdr::ScVal::Vec(Some(stellar_xdr::ScVec(vec))));
}

#[test]
fn test_scval_map_roundtrip() {
    let map: VecM<stellar_xdr::ScMapEntry> = vec![
        stellar_xdr::ScMapEntry {
            key: stellar_xdr::ScVal::I32(1),
            val: stellar_xdr::ScVal::I32(10),
        },
        stellar_xdr::ScMapEntry {
            key: stellar_xdr::ScVal::I32(2),
            val: stellar_xdr::ScVal::I32(20),
        },
    ]
    .try_into()
    .unwrap();
    test_roundtrip(stellar_xdr::ScVal::Map(Some(stellar_xdr::ScMap(map))));

    let map: VecM<stellar_xdr::ScMapEntry> = vec![].try_into().unwrap();
    test_roundtrip(stellar_xdr::ScVal::Map(Some(stellar_xdr::ScMap(map))));
}

#[test]
fn test_scval_address_roundtrip() {
    let contract_id = stellar_xdr::ContractId(stellar_xdr::Hash([0u8; 32]));
    let address = stellar_xdr::ScAddress::Contract(contract_id);
    test_roundtrip(stellar_xdr::ScVal::Address(address));

    let public_key = stellar_xdr::PublicKey::PublicKeyTypeEd25519(stellar_xdr::Uint256([1u8; 32]));
    let address = stellar_xdr::ScAddress::Account(stellar_xdr::AccountId(public_key));
    test_roundtrip(stellar_xdr::ScVal::Address(address));
}

// ─────────────────────────────────────────────────────────────────────────
// SorobanTransactionData roundtrip test
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_soroban_transaction_data_roundtrip() {
    let source = stellar_xdr::MuxedAccount::Ed25519(stellar_xdr::Uint256([0u8; 32]));

    let host_function = stellar_xdr::HostFunction::UploadContractWasm(
        stellar_xdr::BytesM::try_from(vec![1u8, 2u8, 3u8]).unwrap(),
    );

    let invoke_op = stellar_xdr::InvokeHostFunctionOp {
        host_function,
        auth: VecM::<stellar_xdr::SorobanAuthorizationEntry>::default(),
    };

    let operation = stellar_xdr::Operation {
        source_account: None,
        body: stellar_xdr::OperationBody::InvokeHostFunction(invoke_op),
    };

    let operations: VecM<stellar_xdr::Operation, 100> = vec![operation].try_into().unwrap();

    let tx = stellar_xdr::Transaction {
        source_account: source,
        fee: 100,
        seq_num: stellar_xdr::SequenceNumber(1),
        cond: stellar_xdr::Preconditions::None,
        memo: stellar_xdr::Memo::None,
        operations,
        ext: stellar_xdr::TransactionExt::V0,
    };

    let tx_env = stellar_xdr::TransactionEnvelope::Tx(stellar_xdr::TransactionV1Envelope {
        tx,
        signatures: VecM::<stellar_xdr::DecoratedSignature, 20>::default(),
    });

    test_roundtrip(tx_env);
}

// ─────────────────────────────────────────────────────────────────────────
// base64 encoding/decoding helper tests
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_base64_encode_decode_roundtrip() {
    let original = b"hello world";
    let encoded = base64::engine::general_purpose::STANDARD.encode(original);
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&encoded)
        .expect("base64 decode should succeed");
    assert_eq!(original, decoded, "base64 roundtrip should preserve bytes");
}

#[test]
fn test_base64_empty_roundtrip() {
    let original: Vec<u8> = vec![];
    let encoded = base64::engine::general_purpose::STANDARD.encode(&original);
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&encoded)
        .expect("base64 decode should succeed");
    assert_eq!(original, decoded, "empty base64 roundtrip should work");
}

#[test]
fn test_base64_xdr_roundtrip() {
    let value = stellar_xdr::ScVal::I64(42);
    let xdr_bytes = value
        .to_xdr(stellar_xdr::Limits::none())
        .expect("encoding should succeed");

    let encoded = base64::engine::general_purpose::STANDARD.encode(&xdr_bytes);
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&encoded)
        .expect("base64 decode should succeed");

    let decoded_value = stellar_xdr::ScVal::from_xdr(&decoded, stellar_xdr::Limits::none())
        .expect("XDR decode should succeed");

    assert_eq!(
        value, decoded_value,
        "XDR + base64 roundtrip should preserve value"
    );
}

#[test]
fn test_base64_invalid_fails() {
    let invalid = "not-base64!!!";
    let result = base64::engine::general_purpose::STANDARD.decode(invalid);
    assert!(result.is_err(), "invalid base64 should fail to decode");
}
