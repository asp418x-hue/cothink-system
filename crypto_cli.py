#!/usr/bin/env python3
"""
Cryptographic Protocol CLI Tool

Command-line interface for encryption, key derivation, and protocol operations.
"""

import argparse
import sys
import json
import hashlib
import base64
from typing import Optional
from pathlib import Path

from deterministic_crypto import (
    DeterministicEncryption, EncryptionConfig, HashAlgorithm,
    analyze_encryption
)
from crypto_protocols import (
    DeterministicMAC, CryptographicCommitment, CryptographicChain,
    MultiPartyKeyExchange, SimplifiedZKProof, ProtocolAnalyzer
)


def parse_args():
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(
        description="Deterministic Encryption with In-Folding Rhash",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Encrypt text
  crypto_cli.py encrypt --password "secret" --data "message"
  
  # Encrypt file
  crypto_cli.py encrypt --password "secret" --file plaintext.txt
  
  # Decrypt
  crypto_cli.py decrypt --password "secret" --data "<base64_encrypted>"
  
  # Key derivation
  crypto_cli.py kdf --password "secret" --salt "1234567890123456" --length 32
  
  # Compute MAC
  crypto_cli.py mac --message "data" --key "secret_key"
  
  # Create commitment
  crypto_cli.py commit --data "secret"
  
  # Analyze data entropy
  crypto_cli.py analyze entropy --data "test_data"
        """
    )
    
    subparsers = parser.add_subparsers(dest="command", help="Command to execute")
    
    # Encryption command
    encrypt_parser = subparsers.add_parser("encrypt", help="Encrypt data")
    encrypt_parser.add_argument("--password", "-p", required=True,
                               help="Encryption password")
    encrypt_parser.add_argument("--data", "-d", help="Data to encrypt")
    encrypt_parser.add_argument("--file", "-f", help="File to encrypt")
    encrypt_parser.add_argument("--output", "-o", help="Output file")
    encrypt_parser.add_argument("--deterministic", action="store_true",
                               help="Use fixed salt for deterministic output")
    encrypt_parser.add_argument("--format", choices=["base64", "hex"],
                               default="base64", help="Output format")
    
    # Decryption command
    decrypt_parser = subparsers.add_parser("decrypt", help="Decrypt data")
    decrypt_parser.add_argument("--password", "-p", required=True,
                               help="Decryption password")
    decrypt_parser.add_argument("--data", "-d", help="Data to decrypt (base64 or hex)")
    decrypt_parser.add_argument("--file", "-f", help="File to decrypt")
    decrypt_parser.add_argument("--output", "-o", help="Output file")
    decrypt_parser.add_argument("--format", choices=["base64", "hex"],
                               default="base64", help="Input format")
    
    # KDF command
    kdf_parser = subparsers.add_parser("kdf", help="Key derivation function")
    kdf_parser.add_argument("--password", "-p", required=True,
                           help="Password/entropy source")
    kdf_parser.add_argument("--salt", "-s", required=True,
                           help="Salt (hex or string)")
    kdf_parser.add_argument("--length", "-l", type=int, default=32,
                           help="Output key length in bytes")
    kdf_parser.add_argument("--iterations", type=int,
                           help="KDF iterations (default from config)")
    
    # MAC command
    mac_parser = subparsers.add_parser("mac", help="Message authentication code")
    mac_parser.add_argument("--message", "-m", required=True,
                           help="Message to authenticate")
    mac_parser.add_argument("--key", "-k", required=True,
                           help="MAC key")
    mac_parser.add_argument("--verify", "-v", help="MAC to verify")
    mac_parser.add_argument("--format", choices=["base64", "hex"],
                           default="hex", help="Output format")
    
    # Commitment command
    commit_parser = subparsers.add_parser("commit", help="Commitment scheme")
    commit_parser.add_argument("--data", "-d", required=True,
                              help="Data to commit")
    commit_parser.add_argument("--randomness", "-r",
                              help="Randomness (auto-generated if not provided)")
    commit_parser.add_argument("--reveal", action="store_true",
                              help="Reveal commitment")
    
    # Chain command
    chain_parser = subparsers.add_parser("chain", help="Cryptographic chain")
    chain_parser.add_argument("--operations", "-o", required=True,
                             help='JSON array of operations: [["op1", "data1"], ...]')
    
    # Analyze command
    analyze_parser = subparsers.add_parser("analyze", help="Analyze cryptographic data")
    analyze_subparsers = analyze_parser.add_subparsers(dest="analysis_type")
    
    entropy_parser = analyze_subparsers.add_parser("entropy", help="Analyze entropy")
    entropy_parser.add_argument("--data", "-d", required=True,
                               help="Data to analyze")
    
    compare_parser = analyze_subparsers.add_parser("compare",
                                                  help="Compare outputs")
    compare_parser.add_argument("--output1", "-o1", required=True,
                               help="First output (hex)")
    compare_parser.add_argument("--output2", "-o2", required=True,
                               help="Second output (hex)")
    
    # Configuration
    parser.add_argument("--hash-algo", choices=["sha256", "sha512", "sha3_256", "sha3_512"],
                       default="sha256", help="Hash algorithm")
    parser.add_argument("--fold-depth", type=int, default=5,
                       help="In-folding fold depth")
    
    return parser.parse_args()


def encode_output(data: bytes, format_type: str) -> str:
    """Encode output data."""
    if format_type == "hex":
        return data.hex()
    else:  # base64
        return base64.b64encode(data).decode()


def decode_input(data: str, format_type: str) -> bytes:
    """Decode input data."""
    if format_type == "hex":
        return bytes.fromhex(data)
    else:  # base64
        return base64.b64decode(data)


def cmd_encrypt(args):
    """Handle encrypt command."""
    # Get data to encrypt
    if args.data:
        plaintext = args.data.encode()
    elif args.file:
        plaintext = Path(args.file).read_bytes()
    else:
        plaintext = sys.stdin.buffer.read()
    
    # Create encryption instance
    config = EncryptionConfig(
        hash_algo=HashAlgorithm(args.hash_algo),
        fold_depth=args.fold_depth
    )
    enc = DeterministicEncryption(config)
    
    # Encrypt
    password = args.password.encode()
    if args.deterministic:
        encrypted = enc.encrypt_deterministic(plaintext, password)
    else:
        encrypted = enc.encrypt(plaintext, password)
    
    # Encode and output
    output = encode_output(encrypted, args.format)
    
    if args.output:
        Path(args.output).write_text(output)
        print(f"Encrypted to {args.output}", file=sys.stderr)
    else:
        print(output)
    
    # Show stats
    stats = analyze_encryption(plaintext, encrypted, config)
    print(f"Plaintext: {stats.plaintext_size} bytes", file=sys.stderr)
    print(f"Ciphertext: {stats.ciphertext_size} bytes", file=sys.stderr)
    print(f"Expansion: {stats.expansion_ratio:.2f}x", file=sys.stderr)


def cmd_decrypt(args):
    """Handle decrypt command."""
    # Get data to decrypt
    if args.data:
        encrypted = decode_input(args.data, args.format)
    elif args.file:
        encrypted = decode_input(Path(args.file).read_text(), args.format)
    else:
        encrypted = decode_input(sys.stdin.read(), args.format)
    
    # Create decryption instance
    config = EncryptionConfig(
        hash_algo=HashAlgorithm(args.hash_algo),
        fold_depth=args.fold_depth
    )
    dec = DeterministicEncryption(config)
    
    # Decrypt
    try:
        password = args.password.encode()
        plaintext = dec.decrypt(encrypted, password)
        
        if args.output:
            Path(args.output).write_bytes(plaintext)
            print(f"Decrypted to {args.output}", file=sys.stderr)
        else:
            print(plaintext.decode(errors="replace"))
    except Exception as e:
        print(f"Decryption failed: {e}", file=sys.stderr)
        sys.exit(1)


def cmd_kdf(args):
    """Handle KDF command."""
    from deterministic_crypto import DeterministicKDF
    
    config = EncryptionConfig(
        hash_algo=HashAlgorithm(args.hash_algo),
        fold_depth=args.fold_depth
    )
    if args.iterations:
        config.kdf_iterations = args.iterations
    
    kdf = DeterministicKDF(config)
    
    # Parse salt
    try:
        salt = bytes.fromhex(args.salt)
    except ValueError:
        salt = args.salt.encode()
    
    password = args.password.encode()
    key = kdf.derive_key(password, salt, args.length)
    
    output = encode_output(key, "hex")
    print(output)


def cmd_mac(args):
    """Handle MAC command."""
    config = EncryptionConfig(
        hash_algo=HashAlgorithm(args.hash_algo),
        fold_depth=args.fold_depth
    )
    mac_gen = DeterministicMAC(config)
    
    message = args.message.encode()
    key = args.key.encode()
    
    mac = mac_gen.compute_mac(message, key)
    
    if args.verify:
        verify_mac = decode_input(args.verify, args.format)
        if mac_gen.verify_mac(message, verify_mac, key):
            print("MAC verification: PASSED")
        else:
            print("MAC verification: FAILED")
            sys.exit(1)
    else:
        output = encode_output(mac, args.format)
        print(output)


def cmd_commit(args):
    """Handle commitment command."""
    config = EncryptionConfig(
        hash_algo=HashAlgorithm(args.hash_algo),
        fold_depth=args.fold_depth
    )
    commitment_gen = CryptographicCommitment(config)
    
    data = args.data.encode()
    
    if args.reveal and args.randomness:
        randomness = decode_input(args.randomness, "hex")
        verified = commitment_gen.reveal(args.data, data, randomness)
        print(f"Commitment verified: {verified}")
    else:
        comm_id, randomness = commitment_gen.commit(data)
        print(f"Commitment ID: {comm_id}")
        print(f"Randomness: {encode_output(randomness, 'hex')}")


def cmd_chain(args):
    """Handle chain command."""
    config = EncryptionConfig(
        hash_algo=HashAlgorithm(args.hash_algo),
        fold_depth=args.fold_depth
    )
    chain = CryptographicChain(config)
    
    try:
        ops = json.loads(args.operations)
        for op_name, data_str in ops:
            chain.add_operation(op_name, data_str.encode())
        
        proof = chain.get_chain_proof()
        print(json.dumps(proof, indent=2))
    except json.JSONDecodeError as e:
        print(f"Invalid JSON: {e}", file=sys.stderr)
        sys.exit(1)


def cmd_analyze(args):
    """Handle analyze command."""
    analyzer = ProtocolAnalyzer()
    
    if args.analysis_type == "entropy":
        data = args.data.encode()
        result = analyzer.analyze_entropy(data)
        print(json.dumps(result, indent=2))
    
    elif args.analysis_type == "compare":
        out1 = bytes.fromhex(args.output1)
        out2 = bytes.fromhex(args.output2)
        result = analyzer.compare_outputs(out1, out2)
        print(json.dumps(result, indent=2))


def main():
    """Main entry point."""
    args = parse_args()
    
    if not args.command:
        print("Use --help for usage information", file=sys.stderr)
        sys.exit(1)
    
    # Route to command handler
    handlers = {
        "encrypt": cmd_encrypt,
        "decrypt": cmd_decrypt,
        "kdf": cmd_kdf,
        "mac": cmd_mac,
        "commit": cmd_commit,
        "chain": cmd_chain,
        "analyze": cmd_analyze,
    }
    
    handler = handlers.get(args.command)
    if handler:
        try:
            handler(args)
        except Exception as e:
            print(f"Error: {e}", file=sys.stderr)
            sys.exit(1)
    else:
        print(f"Unknown command: {args.command}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
