# Kademlia Blockchain Auction System

This is a Rust prototype of a non-permissioned auction ledger built on top of a secure Kademlia-style peer-to-peer network. It was originally started for the System and Data Security 2024/2025 assignment and has been cleaned up into a portfolio-friendly project that can be built, tested, and demonstrated locally.

The implementation focuses on three assignment themes:

- A public ledger model with hash-addressed blocks and transactions.
- A secure P2P layer using signed gRPC messages, node IDs derived from public keys, Kademlia routing tables, bootstrap challenges, `STORE`, `FIND_NODE`, and `FIND_VALUE`.
- Auction data that can be serialized over protobuf, stored by DHT key, and retrieved by key.

## Current Status

Implemented:

- Rust gRPC service generated from `proto/kademlia.proto`.
- Authenticated message wrapper with RSA signatures.
- Sender identity validation by hashing the sender public key.
- Bootstrap proof-of-work challenge before a peer joins through the bootstrap node.
- Kademlia routing table with k-buckets and XOR-distance lookup.
- Auction creation, deterministic auction keys, protobuf conversion, local storage, and DHT-style `FIND_VALUE` responses.
- Self-contained protobuf build using vendored `protoc`.
- Unit tests for signed messages, auction serialization, node auction storage, and block hashing.

Still prototype-level:

- Consensus is represented by block and proof-of-work primitives, but there is no full replicated chain synchronization or fork-choice implementation yet.
- Proof-of-reputation is not implemented.
- Auction bidding rules are data-model-only; there is no full bid lifecycle, close-auction workflow, or winner settlement yet.
- The application is a peer process, not a polished end-user CLI or web UI.

## Architecture

```text
src/
  main.rs                 process bootstrap: config, keys, node, server/client
  auction.rs              auction and bid domain objects plus protobuf conversion
  node.rs                 local node identity, routing table, and auction store
  g_rpc/
    server.rs             Kademlia gRPC server implementation
    client.rs             bootstrap, challenge, store, find-node, find-value client calls
  routing/
    kbucket.rs            single Kademlia bucket
    routing_table.rs      XOR-distance bucket selection and closest-node queries
  blockchain/
    address.rs            256-bit address wrapper
    block.rs              hash-addressed block structure
    transaction.rs        simple transaction model
  utils/
    config.rs             environment and CLI config
    crypto_own.rs         hashing, RSA signatures, authenticated message handling
    execution.rs          server/client thread runner
```

## Requirements

- Rust stable toolchain.
- OpenSSL development libraries available to the Rust `openssl` crate.

You do not need to install `protoc` manually; the build uses `protoc-bin-vendored`.

## Configuration

Runtime configuration is read from `.env` plus the required `--port` CLI argument.

Important values:

```env
BOOTSTRAP_PEER_IP='127.0.0.1'
BOOTSTRAP_PEER_PORT=8000
MAX_N_KBUCKET_ENTRIES=2
PEER_SYNC_MS=10000
CHALLENGE_DIFFICULTY=20
K_VALUE=3
```

Each node stores its identity under:

```text
keys/<port>/private_key.pem
keys/<port>/public_key.pem
```

Keys are generated automatically on first run and are ignored by git.

For quick local demos, consider lowering `CHALLENGE_DIFFICULTY` to `5` or `8`. The checked-in value `20` is intentionally more expensive.

## Run Tests

```bash
cargo test
```

Expected result:

```text
running 5 tests
test result: ok. 5 passed
```

## Run A Local Network

Open multiple terminals from the repository root.

Terminal 1, bootstrap node:

```bash
cargo run -- --port 8000
```

Terminal 2, peer node:

```bash
cargo run -- --port 8001
```

Terminal 3, another peer:

```bash
cargo run -- --port 8002
```

The bootstrap node skips bootstrapping itself. Other peers request a challenge from the bootstrap node, solve it, insert the bootstrap node into their routing table, run `FIND_NODE`, and then start. The current demo path stores a sample auction from non-bootstrap nodes.

## Protocol Summary

All RPCs accept and return `AuthenticatedMessage`:

- `sender`: node ID, IP, and port.
- `public_key`: DER-encoded public key.
- `signature`: RSA/SHA-256 signature over the protobuf payload.
- `payload`: encoded request or response message.

The receiver verifies:

1. The signature is valid for the payload and public key.
2. The sender ID equals `sha256(public_key)`.
3. The payload decodes into the expected RPC type.

Supported RPCs:

- `Ping`
- `Bootstrap`
- `ChallengeResolution`
- `Store`
- `FindNode`
- `FindValue`

## Notes For Portfolio Review

This repository is best presented as a security/distributed-systems prototype rather than a production auction platform. The strongest parts are the authenticated gRPC transport, Kademlia routing/storage flow, deterministic keying, and testable Rust domain model. The README and `explanation.txt` call out the remaining work honestly so reviewers can see both the implemented system and the engineering judgment around unfinished scope.
