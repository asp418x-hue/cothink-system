"""
Deterministic Encryption Protocol with In-Folding Rhash Refactorization

Implements a deterministic (repeatable) encryption scheme using:
- In-folding recursive hash (rhash) refactorization for key derivation
- AES-256-CTR with deterministic IV generation
- HMAC for authentication
"""

import hashlib
import hmac
import struct
from typing import Tuple, Optional, Dict, Any
from dataclasses import dataclass
from enum import Enum
import os

from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.backends import default_backend
from cryptography.hazmat.primitives import hashes


class HashAlgorithm(Enum):
    """Supported hash algorithms for in-folding rhash."""
    SHA256 = "sha256"
    SHA512 = "sha512"
    SHA3_256 = "sha3_256"
    SHA3_512 = "sha3_512"


@dataclass
class EncryptionConfig:
    """Configuration for deterministic encryption."""
    hash_algo: HashAlgorithm = HashAlgorithm.SHA256
    fold_depth: int = 5  # In-folding recursive depth
    kdf_iterations: int = 100000  # KDF iteration count
    cipher_key_size: int = 32  # 256-bit key for AES-256
    auth_key_size: int = 32  # 256-bit key for HMAC
    salt_size: int = 16  # Salt size in bytes
    version: int = 1  # Protocol version


class InFoldingRhash:
    """
    In-Folding Recursive Hash (Rhash) Refactorization
    
    Performs recursive, layered hashing where each fold incorporates:
    - Previous hash state
    - Current data segment
    - Fold iteration number
    - Refactorization constants
    
    This creates a dependency chain where changing any bit earlier
    affects all subsequent folds deterministically.
    """
    
    def __init__(self, config: EncryptionConfig):
        self.config = config
        self.hash_name = config.hash_algo.value
    
    def _get_hash_func(self):
        """Get hash function based on configuration."""
        return hashlib.new(self.hash_name)
    
    def _refactor_constant(self, fold_idx: int, data_len: int) -> bytes:
        """
        Generate refactorization constant for fold iteration.
        
        Constants are deterministically derived from:
        - Fold index
        - Data length
        - Protocol version
        """
        constant_data = struct.pack(
            "!HHL",
            fold_idx & 0xFFFF,
            (data_len & 0xFFFF),
            self.config.version
        )
        return hashlib.sha256(constant_data).digest()[:8]
    
    def _fold_iteration(self, previous_hash: bytes, data_segment: bytes,
                       fold_idx: int) -> bytes:
        """
        Perform single in-folding iteration.
        
        Each fold combines:
        1. Previous hash state (creates dependency chain)
        2. Current data segment
        3. Fold index (prevents cycle attacks)
        4. Refactorization constant
        """
        hasher = self._get_hash_func()
        
        # Refactorization constant (XORed with fold idx)
        refactor_const = self._refactor_constant(fold_idx, len(data_segment))
        
        # Layer 1: Previous state
        hasher.update(previous_hash)
        
        # Layer 2: Data segment
        hasher.update(data_segment)
        
        # Layer 3: Fold iteration with refactorization
        fold_meta = struct.pack("!I", fold_idx) + refactor_const
        hasher.update(fold_meta)
        
        # Layer 4: Nested hash of previous hash (in-folding characteristic)
        nested_hasher = self._get_hash_func()
        nested_hasher.update(previous_hash)
        nested_hash = nested_hasher.digest()
        hasher.update(nested_hash[:16])
        
        return hasher.digest()
    
    def refactorize(self, data: bytes, salt: Optional[bytes] = None) -> bytes:
        """
        Perform full in-folding rhash refactorization.
        
        Process:
        1. Initialize with data hash
        2. Apply fold_depth recursive folds
        3. Each fold incorporates previous hash + refactorization
        4. Final output is deterministic for same input
        
        Args:
            data: Input data to hash
            salt: Optional salt to incorporate (default: zeros)
        
        Returns:
            Refactorized hash digest
        """
        if salt is None:
            salt = b'\x00' * self.config.salt_size
        
        # Initial hash: combine data with salt
        initial_hasher = self._get_hash_func()
        initial_hasher.update(salt)
        initial_hasher.update(data)
        current_hash = initial_hasher.digest()
        
        # Segment data for folding
        segment_size = len(data) // max(1, self.config.fold_depth) + 1
        
        # Apply recursive in-folding
        for fold_idx in range(self.config.fold_depth):
            # Extract data segment for this fold
            start = (fold_idx * segment_size) % len(data)
            end = min(start + segment_size, len(data))
            segment = data[start:end] if start < len(data) else b''
            
            # Perform fold iteration
            current_hash = self._fold_iteration(current_hash, segment, fold_idx)
        
        return current_hash
    
    def refactorize_with_counter(self, data: bytes, salt: bytes,
                                counter: int) -> bytes:
        """
        Refactorize with counter for key derivation chain.
        
        Enables deriving multiple different keys from same data/salt
        by incorporating counter into the process.
        """
        counter_data = struct.pack("!I", counter)
        combined = data + counter_data
        return self.refactorize(combined, salt)


class DeterministicKDF:
    """
    Deterministic Key Derivation Function using In-Folding Rhash.
    
    Derives keys deterministically from password/entropy using the
    in-folding rhash refactorization algorithm.
    """
    
    def __init__(self, config: EncryptionConfig):
        self.config = config
        self.rhash = InFoldingRhash(config)
    
    def derive_key(self, password: bytes, salt: bytes,
                  key_length: int, iterations: Optional[int] = None) -> bytes:
        """
        Derive key from password using in-folding rhash KDF.
        
        Args:
            password: Password or entropy source
            salt: Salt for KDF
            key_length: Desired key length
            iterations: Number of iterations (default: config.kdf_iterations)
        
        Returns:
            Derived key of requested length
        """
        if iterations is None:
            iterations = self.config.kdf_iterations
        
        # Start with password
        current = password
        
        # Iteratively apply rhash refactorization
        for i in range(iterations):
            current = self.rhash.refactorize(current, salt)
            # Incorporate iteration number to prevent weak iterations
            current = hashlib.sha256(
                current + struct.pack("!I", i)
            ).digest()
        
        # Expand to desired key length if needed
        if key_length <= len(current):
            return current[:key_length]
        
        # Expand using counter-based rhash
        key_material = current
        counter = 0
        while len(key_material) < key_length:
            expansion = self.rhash.refactorize_with_counter(
                current, salt, counter
            )
            key_material += expansion
            counter += 1
        
        return key_material[:key_length]
    
    def derive_cipher_and_auth_keys(self, password: bytes,
                                   salt: bytes) -> Tuple[bytes, bytes]:
        """
        Derive both cipher and authentication keys from password.
        
        Returns:
            (cipher_key, auth_key)
        """
        total_length = (self.config.cipher_key_size +
                       self.config.auth_key_size)
        
        combined = self.derive_key(password, salt, total_length)
        
        cipher_key = combined[:self.config.cipher_key_size]
        auth_key = combined[self.config.cipher_key_size:total_length]
        
        return cipher_key, auth_key
    
    def verify_kdf_consistency(self, password: bytes, salt: bytes) -> bool:
        """
        Verify that KDF produces consistent output (deterministic).
        """
        key1 = self.derive_key(password, salt, 32)
        key2 = self.derive_key(password, salt, 32)
        return key1 == key2


class DeterministicEncryption:
    """
    Deterministic AES-256-CTR Encryption Protocol.
    
    Features:
    - Deterministic IV generation from plaintext (for same plaintext+key)
    - HMAC authentication
    - In-folding rhash for key derivation
    - Versioned format for future compatibility
    """
    
    def __init__(self, config: Optional[EncryptionConfig] = None):
        self.config = config or EncryptionConfig()
        self.kdf = DeterministicKDF(self.config)
        self.rhash = InFoldingRhash(self.config)
    
    def _generate_deterministic_iv(self, salt: bytes) -> bytes:
        """
        Generate deterministic IV from salt.
        
        Same salt produces same IV, enabling deterministic encryption.
        IV is derived using rhash refactorization of salt.
        """
        iv_material = self.rhash.refactorize(salt + b"IV_GEN", salt)
        return iv_material[:16]  # 128-bit IV for AES
    
    def encrypt(self, plaintext: bytes, password: bytes,
               salt: Optional[bytes] = None) -> bytes:
        """
        Encrypt plaintext deterministically.
        
        Format: [version:1][salt:16][ciphertext:n][hmac:32]
        
        Args:
            plaintext: Data to encrypt
            password: Encryption password
            salt: Optional salt (generated if not provided)
        
        Returns:
            Encrypted message with metadata
        """
        if salt is None:
            salt = os.urandom(self.config.salt_size)
        
        # Derive cipher and auth keys
        cipher_key, auth_key = self.kdf.derive_cipher_and_auth_keys(
            password, salt
        )
        
        # Generate deterministic IV
        iv = self._generate_deterministic_iv(salt)
        
        # Encrypt using AES-256-CTR
        cipher = Cipher(
            algorithms.AES(cipher_key),
            modes.CTR(iv),
            backend=default_backend()
        )
        encryptor = cipher.encryptor()
        ciphertext = encryptor.update(plaintext) + encryptor.finalize()
        
        # Build message: version + salt + ciphertext
        message = struct.pack("!B", self.config.version) + salt + ciphertext
        
        # Compute HMAC over message for authentication
        h = hmac.new(auth_key, message, hashlib.sha256)
        auth_tag = h.digest()
        
        # Final format: message + auth_tag
        return message + auth_tag
    
    def decrypt(self, encrypted_message: bytes, password: bytes) -> bytes:
        """
        Decrypt message encrypted with this protocol.
        
        Args:
            encrypted_message: Output from encrypt()
            password: Decryption password
        
        Returns:
            Original plaintext
        
        Raises:
            ValueError: If authentication fails or format is invalid
        """
        if len(encrypted_message) < 1 + self.config.salt_size + 32:
            raise ValueError("Invalid message format")
        
        # Extract components
        version = struct.unpack("!B", encrypted_message[:1])[0]
        if version != self.config.version:
            raise ValueError(f"Unsupported version: {version}")
        
        salt_end = 1 + self.config.salt_size
        salt = encrypted_message[1:salt_end]
        
        hmac_start = len(encrypted_message) - 32
        ciphertext = encrypted_message[salt_end:hmac_start]
        received_hmac = encrypted_message[hmac_start:]
        
        # Derive keys
        cipher_key, auth_key = self.kdf.derive_cipher_and_auth_keys(
            password, salt
        )
        
        # Verify HMAC
        message_part = encrypted_message[:hmac_start]
        h = hmac.new(auth_key, message_part, hashlib.sha256)
        expected_hmac = h.digest()
        
        if not hmac.compare_digest(received_hmac, expected_hmac):
            raise ValueError("Authentication failed")
        
        # Generate deterministic IV
        iv = self._generate_deterministic_iv(salt)
        
        # Decrypt
        cipher = Cipher(
            algorithms.AES(cipher_key),
            modes.CTR(iv),
            backend=default_backend()
        )
        decryptor = cipher.decryptor()
        plaintext = decryptor.update(ciphertext) + decryptor.finalize()
        
        return plaintext
    
    def encrypt_deterministic(self, plaintext: bytes,
                             password: bytes) -> bytes:
        """
        Encrypt with fixed salt for true deterministic output.
        
        Warning: Using fixed salt reduces security. Use only when
        deterministic ciphertext is required.
        """
        fixed_salt = hashlib.sha256(password).digest()[:self.config.salt_size]
        return self.encrypt(plaintext, password, fixed_salt)
    
    def get_ciphertext_hash(self, encrypted_message: bytes) -> str:
        """Get hash of ciphertext portion for analysis."""
        salt_end = 1 + self.config.salt_size
        hmac_start = len(encrypted_message) - 32
        ciphertext = encrypted_message[salt_end:hmac_start]
        return hashlib.sha256(ciphertext).hexdigest()[:16]


@dataclass
class EncryptionStats:
    """Statistics about encryption operation."""
    plaintext_size: int
    ciphertext_size: int
    expansion_ratio: float
    hash_algo: str
    fold_depth: int
    kdf_iterations: int
    deterministic: bool


def analyze_encryption(plaintext: bytes, encrypted: bytes,
                      config: EncryptionConfig) -> EncryptionStats:
    """Analyze encryption operation statistics."""
    salt_end = 1 + config.salt_size
    hmac_start = len(encrypted) - 32
    actual_ciphertext = encrypted[salt_end:hmac_start]
    
    expansion_ratio = len(encrypted) / len(plaintext) if plaintext else 0
    
    return EncryptionStats(
        plaintext_size=len(plaintext),
        ciphertext_size=len(encrypted),
        expansion_ratio=expansion_ratio,
        hash_algo=config.hash_algo.value,
        fold_depth=config.fold_depth,
        kdf_iterations=config.kdf_iterations,
        deterministic=True
    )


# Example usage and testing
if __name__ == "__main__":
    import sys
    
    # Create configuration
    config = EncryptionConfig(
        hash_algo=HashAlgorithm.SHA256,
        fold_depth=5,
        kdf_iterations=100000
    )
    
    # Create encryption instance
    enc = DeterministicEncryption(config)
    
    # Test data
    plaintext = b"This is a secret message that needs deterministic encryption."
    password = b"my_secure_password"
    
    print("=== Deterministic Encryption Test ===\n")
    
    # Encrypt multiple times with same password - should be different (different salt)
    print("1. Encryption with random salt (non-deterministic ciphertext):")
    enc1 = enc.encrypt(plaintext, password)
    enc2 = enc.encrypt(plaintext, password)
    print(f"   Encryption 1 hash: {hashlib.sha256(enc1).hexdigest()[:16]}")
    print(f"   Encryption 2 hash: {hashlib.sha256(enc2).hexdigest()[:16]}")
    print(f"   Different (expected): {enc1 != enc2}\n")
    
    # Decrypt
    dec1 = enc.decrypt(enc1, password)
    print(f"2. Decryption verification:")
    print(f"   Original:  {plaintext}")
    print(f"   Decrypted: {dec1}")
    print(f"   Match: {plaintext == dec1}\n")
    
    # Deterministic encryption
    print("3. Deterministic encryption (fixed salt):")
    det1 = enc.encrypt_deterministic(plaintext, password)
    det2 = enc.encrypt_deterministic(plaintext, password)
    print(f"   Deterministic 1 hash: {hashlib.sha256(det1).hexdigest()[:16]}")
    print(f"   Deterministic 2 hash: {hashlib.sha256(det2).hexdigest()[:16]}")
    print(f"   Same (expected): {det1 == det2}\n")
    
    # KDF consistency
    print("4. KDF Consistency test:")
    kdf = DeterministicKDF(config)
    salt = b"test_salt_16byte"
    is_consistent = kdf.verify_kdf_consistency(password, salt)
    print(f"   KDF produces consistent keys: {is_consistent}\n")
    
    # Statistics
    stats = analyze_encryption(plaintext, enc1, config)
    print("5. Encryption Statistics:")
    print(f"   Plaintext size: {stats.plaintext_size} bytes")
    print(f"   Ciphertext size: {stats.ciphertext_size} bytes")
    print(f"   Expansion ratio: {stats.expansion_ratio:.2f}x")
    print(f"   Hash algorithm: {stats.hash_algo}")
    print(f"   Fold depth: {stats.fold_depth}")
    print(f"   KDF iterations: {stats.kdf_iterations}")
