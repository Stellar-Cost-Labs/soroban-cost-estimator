use base64::Engine;
use soroban_cost_estimator::rpc::simulate::{
    parse_transaction_data_resource_fee, parse_transaction_data_resources,
};
use soroban_cost_estimator::xdr_helper::{
    apply_config_entry, begin_snapshot, build_simulation_tx_envelope, decode_config_entry_xdr,
    parse_arg_scval, parse_contract_id,
};
use stellar_xdr::{
    AccountId, BytesM, ConfigSettingContractBandwidthV0, ConfigSettingContractComputeV0,
    ConfigSettingContractEventsV0, ConfigSettingContractHistoricalDataV0,
    ConfigSettingContractLedgerCostV0, ConfigSettingEntry, ContractDataDurability, ContractId,
    Hash, HostFunction, Int64, LedgerEntryData, LedgerFootprint, LedgerKey, LedgerKeyContractCode,
    LedgerKeyContractData, Limits, OperationBody, PublicKey, ReadXdr, ScAddress, ScMap, ScMapEntry,
    ScString, ScSymbol, ScVal, ScVec, SorobanResources, SorobanTransactionData,
    SorobanTransactionDataExt, StateArchivalSettings, StringM, TransactionEnvelope, Uint256, VecM,
    WriteXdr,
};

fn roundtrip_scval(val: &ScVal) -> ScVal {
    let bytes = val
        .to_xdr(Limits::none())
        .expect("ScVal should serialize to XDR");
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let decoded_bytes = base64::engine::general_purpose::STANDARD
        .decode(&b64)
        .expect("base64 should decode");
    assert_eq!(bytes, decoded_bytes);
    ScVal::from_xdr(&decoded_bytes, Limits::none()).expect("ScVal should deserialize from XDR")
}

#[test]
fn scval_primitive_types_roundtrip_xdr_and_base64() {
    let cases = vec![
        ScVal::Bool(true),
        ScVal::Bool(false),
        ScVal::I32(i32::MIN),
        ScVal::I32(0),
        ScVal::I32(i32::MAX),
        ScVal::U32(0),
        ScVal::U32(u32::MAX),
        ScVal::I64(i64::MIN),
        ScVal::I64(-42),
        ScVal::I64(0),
        ScVal::I64(i64::MAX),
        ScVal::U64(0),
        ScVal::U64(u64::MAX),
    ];

    for val in cases {
        let decoded = roundtrip_scval(&val);
        assert_eq!(decoded, val, "roundtrip mismatch for {val:?}");
    }
}

#[test]
fn scval_symbol_and_string_roundtrip() {
    let sym = ScVal::Symbol(ScSymbol::try_from(b"transfer_tokens".to_vec()).expect("valid symbol"));
    assert_eq!(roundtrip_scval(&sym), sym);

    let empty_str = ScVal::String(ScString::default());
    assert_eq!(roundtrip_scval(&empty_str), empty_str);

    let text_str = ScVal::String(
        StringM::try_from(b"soroban-cost-estimator-xdr-test".to_vec())
            .map(ScString::from)
            .expect("valid string"),
    );
    assert_eq!(roundtrip_scval(&text_str), text_str);
}

#[test]
fn scval_vec_and_map_nested_roundtrip() {
    let vec_items: VecM<ScVal> = vec![
        ScVal::U32(1),
        ScVal::I64(-999_999),
        ScVal::Symbol(ScSymbol::try_from(b"inner".to_vec()).expect("valid symbol")),
    ]
    .try_into()
    .expect("valid VecM");
    let sc_vec = ScVal::Vec(Some(ScVec(vec_items)));
    assert_eq!(roundtrip_scval(&sc_vec), sc_vec);

    let map_entries: VecM<ScMapEntry> = vec![
        ScMapEntry {
            key: ScVal::Symbol(ScSymbol::try_from(b"amount".to_vec()).expect("valid symbol")),
            val: ScVal::I64(5_000_000),
        },
        ScMapEntry {
            key: ScVal::Symbol(ScSymbol::try_from(b"active".to_vec()).expect("valid symbol")),
            val: ScVal::Bool(true),
        },
    ]
    .try_into()
    .expect("valid map entries");
    let sc_map = ScVal::Map(Some(ScMap(map_entries)));
    assert_eq!(roundtrip_scval(&sc_map), sc_map);
}

#[test]
fn scval_address_contract_and_account_roundtrip() {
    let contract_addr = ScVal::Address(ScAddress::Contract(ContractId(Hash([0xAB; 32]))));
    assert_eq!(roundtrip_scval(&contract_addr), contract_addr);

    let account_addr = ScVal::Address(ScAddress::Account(AccountId(
        PublicKey::PublicKeyTypeEd25519(Uint256([0xCD; 32])),
    )));
    assert_eq!(roundtrip_scval(&account_addr), account_addr);
}

#[test]
fn parse_arg_scval_inference_and_xdr_roundtrip() {
    let b_true = parse_arg_scval("flag=true");
    assert_eq!(b_true, ScVal::Bool(true));
    assert_eq!(roundtrip_scval(&b_true), b_true);

    let b_false = parse_arg_scval("false");
    assert_eq!(b_false, ScVal::Bool(false));
    assert_eq!(roundtrip_scval(&b_false), b_false);

    let i64_val = parse_arg_scval("step=-123456789");
    assert_eq!(i64_val, ScVal::I64(-123_456_789));
    assert_eq!(roundtrip_scval(&i64_val), i64_val);

    let u64_val = parse_arg_scval("u64_max=18446744073709551615");
    assert_eq!(u64_val, ScVal::U64(u64::MAX));
    assert_eq!(roundtrip_scval(&u64_val), u64_val);

    let str_val = parse_arg_scval("recipient=alice_account");
    let decoded = roundtrip_scval(&str_val);
    assert_eq!(decoded, str_val);
}

#[test]
fn soroban_transaction_data_roundtrip_resources_and_fee() {
    let ro_key = LedgerKey::ContractCode(LedgerKeyContractCode {
        hash: Hash([0x11; 32]),
    });
    let rw_key = LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::Contract(ContractId(Hash([0x22; 32]))),
        key: ScVal::Symbol(ScSymbol::try_from(b"COUNTER".to_vec()).expect("valid symbol")),
        durability: ContractDataDurability::Persistent,
    });

    let tx_data = SorobanTransactionData {
        ext: SorobanTransactionDataExt::V0,
        resources: SorobanResources {
            footprint: LedgerFootprint {
                read_only: vec![ro_key.clone()].try_into().expect("valid ro VecM"),
                read_write: vec![rw_key].try_into().expect("valid rw VecM"),
            },
            instructions: 4_567_890,
            disk_read_bytes: 2_048,
            write_bytes: 512,
        },
        resource_fee: 189_450,
    };

    let xdr_bytes = tx_data
        .to_xdr(Limits::none())
        .expect("SorobanTransactionData should encode");
    let b64 = Some(base64::engine::general_purpose::STANDARD.encode(&xdr_bytes));

    let fee = parse_transaction_data_resource_fee(&b64)
        .expect("fee should decode")
        .expect("fee should be Some");
    assert_eq!(fee, 189_450);

    let resources = parse_transaction_data_resources(&b64)
        .expect("resources should decode")
        .expect("resources should be Some");
    assert_eq!(resources.cpu_insns, 4_567_890);
    assert_eq!(resources.read_entries, 1);
    assert_eq!(resources.write_entries, 1);
    assert_eq!(resources.read_bytes, 2_048);
    assert_eq!(resources.write_bytes, 512);
}

#[test]
#[allow(clippy::too_many_lines)]
fn config_setting_entries_roundtrip_through_decode_and_apply() {
    let entries = vec![
        ConfigSettingEntry::ContractComputeV0(ConfigSettingContractComputeV0 {
            ledger_max_instructions: 580_000_000,
            tx_max_instructions: 100_000_000,
            fee_rate_per_instructions_increment: 25,
            tx_memory_limit: 41_943_040,
        }),
        ConfigSettingEntry::ContractLedgerCostV0(ConfigSettingContractLedgerCostV0 {
            ledger_max_disk_read_entries: 200,
            ledger_max_disk_read_bytes: 200_000,
            ledger_max_write_ledger_entries: 100,
            ledger_max_write_bytes: 100_000,
            tx_max_disk_read_entries: 40,
            tx_max_disk_read_bytes: 131_072,
            tx_max_write_ledger_entries: 25,
            tx_max_write_bytes: 65_536,
            fee_disk_read_ledger_entry: 6_250,
            fee_write_ledger_entry: 10_000,
            fee_disk_read1_kb: 1_786,
            soroban_state_target_size_bytes: 100_000_000,
            rent_fee1_kb_soroban_state_size_low: 1_000,
            rent_fee1_kb_soroban_state_size_high: 10_000,
            soroban_state_rent_fee_growth_factor: 5_000,
        }),
        ConfigSettingEntry::ContractHistoricalDataV0(ConfigSettingContractHistoricalDataV0 {
            fee_historical1_kb: 300,
        }),
        ConfigSettingEntry::ContractEventsV0(ConfigSettingContractEventsV0 {
            tx_max_contract_events_size_bytes: 8_192,
            fee_contract_events1_kb: 200,
        }),
        ConfigSettingEntry::ContractBandwidthV0(ConfigSettingContractBandwidthV0 {
            ledger_max_txs_size_bytes: 266_240,
            tx_max_size_bytes: 71_680,
            fee_tx_size1_kb: 1_624,
        }),
        ConfigSettingEntry::StateArchival(StateArchivalSettings {
            max_entry_ttl: 3_110_400,
            min_temporary_ttl: 16,
            min_persistent_ttl: 120_960,
            persistent_rent_rate_denominator: 2_103,
            temp_rent_rate_denominator: 4_206,
            max_entries_to_archive: 1_000,
            live_soroban_state_size_window_sample_size: 30,
            live_soroban_state_size_window_sample_period: 64,
            eviction_scan_size: 100_000,
            starting_eviction_scan_level: 7,
        }),
    ];

    let mut snap = begin_snapshot("testnet", 55_000);
    for expected in entries {
        let ledger_data = LedgerEntryData::ConfigSetting(expected.clone());
        let raw_bytes = ledger_data
            .to_xdr(Limits::none())
            .expect("LedgerEntryData should serialize");
        let b64 = base64::engine::general_purpose::STANDARD.encode(&raw_bytes);

        let decoded =
            decode_config_entry_xdr(&b64, false).expect("decode_config_entry_xdr should succeed");
        assert_eq!(decoded, expected);

        apply_config_entry(&mut snap, decoded);
    }

    assert_eq!(
        snap.contract_compute
            .as_ref()
            .expect("compute set")
            .fee_rate_per_instructions_increment,
        25
    );
    assert_eq!(
        snap.contract_ledger_cost
            .as_ref()
            .expect("ledger cost set")
            .fee_write_ledger_entry,
        10_000
    );
    assert_eq!(
        snap.contract_historical_data
            .as_ref()
            .expect("historical set")
            .fee_historical1_kb,
        300
    );
    assert_eq!(
        snap.contract_events
            .as_ref()
            .expect("events set")
            .fee_contract_events1_kb,
        200
    );
    assert_eq!(
        snap.contract_bandwidth
            .as_ref()
            .expect("bandwidth set")
            .fee_tx_size1_kb,
        1_624
    );
    assert_eq!(
        snap.state_archival
            .as_ref()
            .expect("archival set")
            .persistent_rent_rate_denominator,
        2_103
    );
}

#[test]
fn decode_config_entry_xdr_rejects_invalid_inputs() {
    assert!(decode_config_entry_xdr("!!!not_base64!!!", false).is_err());

    let bad_xdr_b64 = base64::engine::general_purpose::STANDARD.encode([0xFF, 0x00, 0x01]);
    assert!(decode_config_entry_xdr(&bad_xdr_b64, false).is_err());

    let non_config = LedgerEntryData::ContractCode(stellar_xdr::ContractCodeEntry {
        ext: stellar_xdr::ContractCodeEntryExt::V0,
        hash: Hash([0x55; 32]),
        code: BytesM::try_from(vec![0x00, 0x61, 0x73, 0x6d]).expect("valid bytes"),
    });
    let non_config_b64 = base64::engine::general_purpose::STANDARD
        .encode(non_config.to_xdr(Limits::none()).expect("serializes"));
    let err = decode_config_entry_xdr(&non_config_b64, false)
        .expect_err("non-ConfigSetting entry must fail");
    assert!(
        err.to_string().contains("expected ConfigSetting entry"),
        "unexpected error: {err}"
    );
}

#[test]
fn simulation_tx_envelope_upload_and_invoke_roundtrip() {
    let wasm = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let upload_xdr =
        build_simulation_tx_envelope(&wasm, None, None, &[]).expect("upload envelope should build");
    let decoded_upload = TransactionEnvelope::from_xdr(&upload_xdr, Limits::none())
        .expect("upload envelope should decode");
    match decoded_upload {
        TransactionEnvelope::Tx(v1) => {
            assert_eq!(v1.tx.operations.len(), 1);
            match &v1.tx.operations[0].body {
                OperationBody::InvokeHostFunction(op) => match &op.host_function {
                    HostFunction::UploadContractWasm(bytes) => {
                        assert_eq!(bytes.as_slice(), &wasm);
                    }
                    other => panic!("expected UploadContractWasm, got {other:?}"),
                },
                other => panic!("expected InvokeHostFunction, got {other:?}"),
            }
        }
        other => panic!("expected TransactionEnvelope::Tx, got {other:?}"),
    }

    let contract_hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let args = vec![ScVal::I64(42), ScVal::Bool(true)];
    let invoke_xdr =
        build_simulation_tx_envelope(&wasm, Some(contract_hex), Some("increment"), &args)
            .expect("invoke envelope should build");

    let decoded_invoke = TransactionEnvelope::from_xdr(&invoke_xdr, Limits::none())
        .expect("invoke envelope should decode");
    match decoded_invoke {
        TransactionEnvelope::Tx(v1) => match &v1.tx.operations[0].body {
            OperationBody::InvokeHostFunction(op) => match &op.host_function {
                HostFunction::InvokeContract(invoke_args) => {
                    assert_eq!(
                        invoke_args.function_name,
                        ScSymbol::try_from(b"increment".to_vec()).expect("valid symbol")
                    );
                    assert_eq!(invoke_args.args.as_slice(), args.as_slice());
                    let expected_bytes = parse_contract_id(contract_hex).expect("valid hex id");
                    assert_eq!(
                        invoke_args.contract_address,
                        ScAddress::Contract(ContractId(Hash(expected_bytes)))
                    );
                }
                other => panic!("expected InvokeContract, got {other:?}"),
            },
            other => panic!("expected InvokeHostFunction, got {other:?}"),
        },
        other => panic!("expected TransactionEnvelope::Tx, got {other:?}"),
    }
}

#[test]
fn min_resource_fee_int64_xdr_roundtrip() {
    let raw_fee: Int64 = 15_427;
    let bytes = raw_fee.to_xdr(Limits::none()).expect("Int64 should encode");
    let decoded = Int64::from_xdr(&bytes, Limits::none()).expect("Int64 should decode");
    assert_eq!(decoded, 15_427);
}
