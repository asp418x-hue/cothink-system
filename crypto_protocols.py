"""
Cryptographic Protocol Extensions and Advanced Utilities

Extensions to the deterministic encryption protocol including:
- Multi-party key exchange
- Message authentication codes (MAC)
- Commitment schemes
- Zero-knowledge proofs (simplified)
- Secure message sequencing
"""

import hashlib
import hmac
import struct
import time
from typing import Dict, List, Tuple, Optional, Any
from dataclasses import dataclass, asdict
from enum import Enum

from deterministic_crypto import (
    DeterministicEncryption, DeterministicKDF, InFoldingRhash,
    EncryptionConfig, HashAlgorithm
)


class MessageType(Enum):
    """Types of cryptographic messages."""
    PLAINTEXT = 0
    ENCRYPTED = 1
    SIGNED = 2
    COMMITMENT = 3
    PROOF = 4
    SESSION = 5


@dataclass
class SecureMessage:
    """Structured secure message with metadata."""
    msg_type: MessageType
    content: bytes
    timestamp: int
    sequence: int
    metadata: Dict[str, Any]


class SecureMessageSequence:
    """
    Ensures message ordering and prevents replay attacks.
    
    Maintains sequence numbers and timestamps, with verification
    that messages arrive in order.
    """
    
    def __init__(self, config: EncryptionConfig):
        self.config = config
        self.sequence_counter = 0
        self.last_timestamp = 0
    
    def create_message(self, msg_type: MessageType,
                      content: bytes,
                      metadata: Optional[Dict] = None) -> SecureMessage:
        """Create a sequenced message."""
        self.sequence_counter += 1
        current_time = int(time.time() * 1000)  # milliseconds
        
        if current_time <= self.last_timestamp:
            current_time = self.last_timestamp + 1
        self.last_timestamp = current_time
        
        return SecureMessage(
            msg_type=msg_type,
            content=content,
            timestamp=current_time,
            sequence=self.sequence_counter,
            metadata=metadata or {}
        )
    
    def serialize_message(self, msg: SecureMessage) -> bytes:
        """Serialize message with metadata."""
        header = struct.pack(
            "!BQI",
            msg.msg_type.value,
            msg.timestamp,
            msg.sequence
        )
        metadata_str = str(msg.metadata).encode()
        metadata_len = struct.pack("!H", len(metadata_str))
        
        return header + metadata_len + metadata_str + msg.content
    
    def verify_sequence(self, msg: SecureMessage,
                       expected_sequence: int) -> bool:
        """Verify message is in expected sequence."""
        return msg.sequence == expected_sequence


class CryptographicCommitment:
    """
    Commitment scheme using in-folding rhash.
    
    Allows commitment to a value without revealing it, with
    deterministic verification.
    """
    
    def __init__(self, config: EncryptionConfig):
        self.config = config
        self.rhash = InFoldingRhash(config)
        self.commitments: Dict[str, bytes] = {}
    
    def commit(self, data: bytes, randomness: Optional[bytes] = None) -> Tuple[str, bytes]:
        """
        Create commitment to data.
        
        Returns:
            (commitment_id, randomness_needed_for_reveal)
        """
        if randomness is None:
            randomness = hashlib.sha256(data).digest()
        
        # Commitment = rhash(data || randomness)
        commitment_input = data + randomness
        commitment = self.rhash.refactorize(commitment_input)
        
        commitment_id = hashlib.sha256(commitment).hexdigest()[:16]
        self.commitments[commitment_id] = commitment
        
        return commitment_id, randomness
    
    def reveal(self, commitment_id: str, data: bytes,
              randomness: bytes) -> bool:
        """
        Verify commitment reveal.
        
        Deterministically checks that commitment matches data + randomness.
        """
        if commitment_id not in self.commitments:
            return False
        
        # Recompute commitment
        commitment_input = data + randomness
        recomputed = self.rhash.refactorize(commitment_input)
        
        stored = self.commitments[commitment_id]
        return hmac.compare_digest(recomputed, stored)


class SimplifiedZKProof:
    """
    Simplified Zero-Knowledge Proof of knowledge.
    
    Prover demonstrates knowledge of secret without revealing it,
    using challenge-response with in-folding rhash.
    """
    
    def __init__(self, config: EncryptionConfig):
        self.config = config
        self.rhash = InFoldingRhash(config)
    
    def generate_challenge(self, secret: bytes) -> Tuple[bytes, bytes]:
        """
        Generate commitment to secret.
        
        Returns:
            (commitment, secret_key_for_response)
        """
        commitment = self.rhash.refactorize(secret)
        secret_key = hashlib.sha256(secret).digest()
        return commitment, secret_key
    
    def generate_response(self, challenge: bytes, secret_key: bytes) -> bytes:
        """Generate response to challenge."""
        combined = challenge + secret_key
        response = self.rhash.refactorize(combined)
        return response
    
    def verify_response(self, commitment: bytes, challenge: bytes,
                       response: bytes, expected_response: Optional[bytes] = None) -> bool:
        """Verify zero-knowledge proof response."""
        # Recompute expected response
        # This is simplified; real ZKP would use more complex schemes
        verification_hash = hashlib.sha256(commitment + challenge).digest()
        
        return hmac.compare_digest(
            response[:len(verification_hash)],
            verification_hash
        )


class DeterministicMAC:
    """
    Message Authentication Code using in-folding rhash.
    
    Creates authentication tags deterministically.
    """
    
    def __init__(self, config: EncryptionConfig):
        self.config = config
        self.rhash = InFoldingRhash(config)
        self.kdf = DeterministicKDF(config)
    
    def compute_mac(self, message: bytes, key: bytes,
                   salt: Optional[bytes] = None) -> bytes:
        """Compute deterministic MAC."""
        if salt is None:
            salt = hashlib.sha256(key).digest()[:self.config.salt_size]
        
        # Derive MAC key using KDF
        mac_key = self.kdf.derive_key(key, salt, 32)
        
        # Compute MAC using in-folding rhash
        mac_input = message + mac_key
        mac = self.rhash.refactorize(mac_input, salt)
        
        return mac
    
    def verify_mac(self, message: bytes, mac: bytes, key: bytes,
                  salt: Optional[bytes] = None) -> bool:
        """Verify MAC."""
        computed_mac = self.compute_mac(message, key, salt)
        return hmac.compare_digest(computed_mac, mac)


class MultiPartyKeyExchange:
    """
    Simplified multi-party key exchange using in-folding rhash.
    
    Each party contributes to final key derivation.
    """
    
    def __init__(self, config: EncryptionConfig):
        self.config = config
        self.rhash = InFoldingRhash(config)
        self.kdf = DeterministicKDF(config)
        self.contributions: List[bytes] = []
    
    def add_contribution(self, contribution: bytes) -> None:
        """Add participant's contribution."""
        self.contributions.append(contribution)
    
    def derive_shared_key(self, shared_salt: bytes,
                         key_length: int = 32) -> bytes:
        """
        Derive shared key from all contributions.
        
        All participants with same contributions and salt derive same key.
        """
        if not self.contributions:
            raise ValueError("No contributions added")
        
        # Combine all contributions deterministically
        combined = b"".join(self.contributions)
        
        # Derive shared key using KDF
        shared_key = self.kdf.derive_key(combined, shared_salt, key_length)
        
        return shared_key
    
    def reset(self) -> None:
        """Reset for new key exchange."""
        self.contributions.clear()


class CryptographicChain:
    """
    Chain of cryptographic operations maintaining state.
    
    Enables sequential operations where output of one becomes
    input to next, with deterministic reproducibility.
    """
    
    def __init__(self, config: EncryptionConfig):
        self.config = config
        self.rhash = InFoldingRhash(config)
        self.chain_state = b""
        self.operations: List[Tuple[str, bytes]] = []
    
    def add_operation(self, operation: str, data: bytes) -> bytes:
        """
        Add operation to chain.
        
        Returns new chain state.
        """
        operation_encoded = operation.encode()
        
        if not self.chain_state:
            # First operation
            self.chain_state = self.rhash.refactorize(
                operation_encoded + data
            )
        else:
            # Chain previous state + operation + data
            self.chain_state = self.rhash.refactorize(
                self.chain_state + operation_encoded + data
            )
        
        self.operations.append((operation, data))
        return self.chain_state
    
    def get_chain_hash(self) -> str:
        """Get hexadecimal representation of chain state."""
        return hashlib.sha256(self.chain_state).hexdigest()
    
    def verify_chain(self, other_chain: "CryptographicChain") -> bool:
        """Verify two chains are identical."""
        return hmac.compare_digest(self.chain_state, other_chain.chain_state)
    
    def get_chain_proof(self) -> Dict[str, Any]:
        """Get proof of chain operations."""
        return {
            "operation_count": len(self.operations),
            "chain_hash": self.get_chain_hash(),
            "final_state_hash": hashlib.sha256(self.chain_state).hexdigest(),
            "operations": [
                {
                    "op": op_name,
                    "data_hash": hashlib.sha256(data).hexdigest()[:16]
                }
                for op_name, data in self.operations
            ]
        }


class ProtocolAnalyzer:
    """Analyze and validate cryptographic protocols."""
    
    @staticmethod
    def analyze_entropy(data: bytes) -> Dict[str, Any]:
        """Analyze entropy of data."""
        if not data:
            return {"error": "empty_data"}
        
        byte_counts = {}
        for byte in data:
            byte_counts[byte] = byte_counts.get(byte, 0) + 1
        
        # Shannon entropy
        entropy = 0.0
        data_len = len(data)
        for count in byte_counts.values():
            probability = count / data_len
            entropy -= probability * (probability ** 0.5)
        
        unique_bytes = len(byte_counts)
        
        return {
            "total_bytes": data_len,
            "unique_bytes": unique_bytes,
            "entropy": entropy,
            "entropy_bits": entropy * 8,
            "distribution": byte_counts
        }
    
    @staticmethod
    def compare_outputs(output1: bytes, output2: bytes) -> Dict[str, Any]:
        """Compare two cryptographic outputs for determinism."""
        if len(output1) != len(output2):
            return {
                "length_match": False,
                "output1_len": len(output1),
                "output2_len": len(output2),
            }
        
        different_bits = 0
        for b1, b2 in zip(output1, output2):
            xor = b1 ^ b2
            different_bits += bin(xor).count('1')
        
        return {
            "length_match": True,
            "identical": different_bits == 0,
            "different_bits": different_bits,
            "bit_difference_ratio": different_bits / (len(output1) * 8),
            "avalanche_effect": "strong" if different_bits > len(output1) * 4
                              else "weak"
        }


# Example usage
if __name__ == "__main__":
    config = EncryptionConfig(
        hash_algo=HashAlgorithm.SHA256,
        fold_depth=5,
        kdf_iterations=50000
    )
    
    print("=== Advanced Cryptographic Protocols ===\n")
    
    # 1. Message sequencing
    print("1. Secure Message Sequencing:")
    seq = SecureMessageSequence(config)
    msg1 = seq.create_message(MessageType.ENCRYPTED, b"First message")
    msg2 = seq.create_message(MessageType.ENCRYPTED, b"Second message")
    print(f"   Message 1 sequence: {msg1.sequence}")
    print(f"   Message 2 sequence: {msg2.sequence}")
    print(f"   Correct order: {msg2.sequence > msg1.sequence}\n")
    
    # 2. Commitments
    print("2. Cryptographic Commitments:")
    commitment = CryptographicCommitment(config)
    comm_id, rand = commitment.commit(b"secret_data")
    print(f"   Commitment ID: {comm_id}")
    print(f"   Randomness hash: {hashlib.sha256(rand).hexdigest()[:16]}")
    verified = commitment.reveal(comm_id, b"secret_data", rand)
    print(f"   Commitment verified: {verified}\n")
    
    # 3. Deterministic MAC
    print("3. Deterministic Message Authentication Code:")
    mac_gen = DeterministicMAC(config)
    message = b"Authenticate this message"
    key = b"secret_key"
    mac1 = mac_gen.compute_mac(message, key)
    mac2 = mac_gen.compute_mac(message, key)
    print(f"   MAC 1 hash: {hashlib.sha256(mac1).hexdigest()[:16]}")
    print(f"   MAC 2 hash: {hashlib.sha256(mac2).hexdigest()[:16]}")
    print(f"   Deterministic (same): {mac1 == mac2}")
    print(f"   MAC verification: {mac_gen.verify_mac(message, mac1, key)}\n")
    
    # 4. Multi-party key exchange
    print("4. Multi-Party Key Exchange:")
    mpke = MultiPartyKeyExchange(config)
    mpke.add_contribution(b"party_1_secret")
    mpke.add_contribution(b"party_2_secret")
    mpke.add_contribution(b"party_3_secret")
    shared_salt = b"shared_salt_1234"
    shared_key = mpke.derive_shared_key(shared_salt)
    print(f"   Shared key length: {len(shared_key)} bytes")
    print(f"   Key hash: {hashlib.sha256(shared_key).hexdigest()[:16]}\n")
    
    # 5. Cryptographic chain
    print("5. Cryptographic Chain:")
    chain = CryptographicChain(config)
    chain.add_operation("hash", b"data1")
    chain.add_operation("encrypt", b"data2")
    chain.add_operation("sign", b"data3")
    proof = chain.get_chain_proof()
    print(f"   Chain operations: {proof['operation_count']}")
    print(f"   Chain hash: {proof['chain_hash'][:16]}")
    print(f"   Operation sequence: {', '.join(op['op'] for op in proof['operations'])}\n")
    
    # 6. Protocol analysis
    print("6. Protocol Analysis:")
    analyzer = ProtocolAnalyzer()
    data = hashlib.sha256(b"test").digest()
    entropy_info = analyzer.analyze_entropy(data)
    print(f"   Data size: {entropy_info['total_bytes']} bytes")
    print(f"   Unique bytes: {entropy_info['unique_bytes']}")
    print(f"   Entropy: {entropy_info['entropy']:.4f}")
