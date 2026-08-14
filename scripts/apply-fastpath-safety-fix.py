#!/usr/bin/env python3
from pathlib import Path


def replace(path: str, old: str, new: str) -> None:
    file_path = Path(path)
    text = file_path.read_text(encoding="utf-8")
    if old not in text:
        raise SystemExit(f"expected text not found in {path}: {old!r}")
    file_path.write_text(text.replace(old, new), encoding="utf-8")


replace(
    "crates/flow/src/session.rs",
    "    decode_new_txhashes_payload, decode_ping_payload, decode_pong_payload,\n"
    "    decode_txs_request_payload, decode_txs_response_payload, encode_hello_payload,\n",
    "    decode_new_txhashes_payload, decode_ping_payload, decode_pong_payload,\n"
    "    decode_txs_request_payload, encode_hello_payload,\n",
)
replace(
    "crates/flow/src/session.rs",
    "use alephium_protocol::message::{\n"
    "    FrameDecode, NetworkMagic, decode_frame_prefix, decode_raw_message_data, encode_frame,\n"
    "    encode_raw_message_data,\n"
    "};\n",
    "use alephium_protocol::message::{\n"
    "    FrameDecode, NetworkMagic, decode_frame_prefix, decode_raw_message_data,\n"
    "    decode_single_txs_response_payload, encode_frame, encode_raw_message_data,\n"
    "};\n",
)
replace(
    "crates/flow/src/session.rs",
    "            config.max_inflight_tx_requests,\n"
    "            config.tx_request_batch_size,\n"
    "            config.max_tracked_tx_ids,\n",
    "            config.max_inflight_tx_requests,\n"
    "            config.max_tracked_tx_ids,\n",
)
replace(
    "crates/flow/src/session.rs",
    "                let response = decode_txs_response_payload(payload)?.finish(payload_len)?;\n",
    "                let response =\n"
    "                    decode_single_txs_response_payload(payload)?.finish(payload_len)?;\n",
)
replace(
    "crates/flow/src/session.rs",
    "    if completion.unknown_request {\n"
    "        eprintln!(\"ignored unsolicited TxsResponse id={response_id}\");\n"
    "        return;\n"
    "    }\n\n"
    "    let observed_at = unix_time_ns().unwrap_or_default();\n",
    "    if completion.unknown_request {\n"
    "        eprintln!(\"ignored unsolicited TxsResponse id={response_id}\");\n"
    "        return;\n"
    "    }\n"
    "    if completion.malformed_response {\n"
    "        eprintln!(\"rejected malformed TxsResponse id={response_id}\");\n"
    "        return;\n"
    "    }\n\n"
    "    let observed_at = unix_time_ns().unwrap_or_default();\n",
)
replace(
    "crates/flow/src/session.rs",
    "        let (chain_from, chain_to) = transaction\n"
    "            .chain_index\n"
    "            .map(|chain| (chain.from, chain.to))\n"
    "            .unwrap_or((-1, -1));\n",
    "        let chain_from = transaction.chain_index.from;\n"
    "        let chain_to = transaction.chain_index.to;\n",
)

replace(
    "crates/conf/src/lib.rs",
    "pub const DEFAULT_TX_REQUEST_BATCH_SIZE: usize = 64;",
    "pub const DEFAULT_TX_REQUEST_BATCH_SIZE: usize = 1;",
)
replace(
    "crates/conf/src/lib.rs",
    "        self.tx_request_batch_size = value.max(1);",
    "        self.tx_request_batch_size = value.min(DEFAULT_TX_REQUEST_BATCH_SIZE).max(1);",
)

replace(
    "crates/app/src/main.rs",
    "    println!(\"  --tx-batch-size <n>           maximum transaction IDs per request\");",
    "    println!(\"  --tx-batch-size <n>           must be 1 until typed tx parsing is complete\");",
)
replace(
    "crates/app/src/main.rs",
    "            \"--tx-batch-size\" => {\n"
    "                let value = parse_usize(\"--tx-batch-size\", args.next());\n"
    "                config = config.set_tx_request_batch_size(value);\n"
    "            }\n",
    "            \"--tx-batch-size\" => {\n"
    "                let value = parse_usize(\"--tx-batch-size\", args.next());\n"
    "                if value != 1 {\n"
    "                    eprintln!(\"--tx-batch-size must be 1 for canonical raw decoding\");\n"
    "                    process::exit(2);\n"
    "                }\n"
    "                config = config.set_tx_request_batch_size(value);\n"
    "            }\n",
)

replace(
    "docs/NODE_FOR_BOT.md",
    "tx_request_batch_size    = 64",
    "tx_request_batch_size    = 1",
)
replace(
    "docs/BOT_INTEGRATION.md",
    "  --tx-batch-size 64 \\",
    "  --tx-batch-size 1 \\",
)
replace(
    "docs/BOT_INTEGRATION.md",
    "1. Verify `blake2b256(payload)` equals `transaction_id` using the same Alephium transaction-ID semantics.",
    "1. Decode the canonical `TransactionTemplate`, recompute `unsigned.id`, and require it to equal `transaction_id`; hashing the complete template bytes is incorrect.",
)
