"""
Cryptographic Protocol Test Suite and Integration Examples

Comprehensive tests and practical integration examples for the deterministic
encryption protocol and advanced cryptographic utilities.
"""

import hashlib
import time
import json
from typing import Dict, List, Any

from deterministic_crypto import (
    DeterministicEncryption, DeterministicKDF, InFoldingRhash,
    EncryptionConfig, HashAlgorithm, analyze_encryption
)
from crypto_protocols import (
    DeterministicMAC, CryptographicCommitment, CryptographicChain,
    MultiPartyKeyExchange, SimplifiedZKProof, ProtocolAnalyzer,
    SecureMessageSequence, MessageType
)


class CryptoTestSuite:
    """Comprehensive test suite for cryptographic protocols."""
    
    def __init__(self):
        self.config = EncryptionConfig(
            hash_algo=HashAlgorithm.SHA256,
            fold_depth=5,
            kdf_iterations=10000  # Reduced for testing
        )
        self.passed = 0
        self.failed = 0
        self.results = {}
    
    def assert_equal(self, actual: Any, expected: Any, test_name: str) -> bool:
        """Assert values are equal."""
        if actual == expected:
            self.passed += 1
            self._record_test(test_name, True, "")
            return True
        else:
            self.failed += 1
            error = f"Expected {expected}, got {actual}"
            self._record_test(test_name, False, error)
            return False
    
    def assert_true(self, condition: bool, test_name: str) -> bool:
        """Assert condition is true."""
        if condition:
            self.passed += 1
            self._record_test(test_name, True, "")
            return True
        else:
            self.failed += 1
            self._record_test(test_name, False, "Condition was False")
            return False
    
    def assert_different(self, val1: Any, val2: Any, test_name: str) -> bool:
        """Assert values are different."""
        if val1 != val2:
            self.passed += 1
            self._record_test(test_name, True, "")
            return True
        else:
            self.failed += 1
            self._record_test(test_name, False, "Values were equal")
            return False
    
    def _record_test(self, name: str, passed: bool, error: str):
        """Record test result."""
        self.results[name] = {"passed": passed, "error": error}
    
    def report(self) -> Dict[str, Any]:
        """Generate test report."""
        total = self.passed + self.failed
        return {
            "total": total,
            "passed": self.passed,
            "failed": self.failed,
            "pass_rate": (self.passed / total * 100) if total > 0 else 0,
            "results": self.results
        }
    
    # Test methods
    
    def test_kdf_determinism(self):
        """Test KDF produces deterministic keys."""
        kdf = DeterministicKDF(self.config)
        password = b"test_password"
        salt = b"test_salt_16byte"
        
        key1 = kdf.derive_key(password, salt, 32)
        key2 = kdf.derive_key(password, salt, 32)
        
        self.assert_equal(key1, key2, "KDF_determinism")
    
    def test_kdf_sensitivity(self):
        """Test KDF is sensitive to password changes."""
        kdf = DeterministicKDF(self.config)
        salt = b"test_salt_16byte"
        
        key1 = kdf.derive_key(b"password1", salt, 32)
        key2 = kdf.derive_key(b"password2", salt, 32)
        
        self.assert_different(key1, key2, "KDF_sensitivity_to_password")
    
    def test_kdf_salt_sensitivity(self):
        """Test KDF is sensitive to salt changes."""
        kdf = DeterministicKDF(self.config)
        password = b"password"
        
        key1 = kdf.derive_key(password, b"salt_1__16byte1", 32)
        key2 = kdf.derive_key(password, b"salt_2__16byte2", 32)
        
        self.assert_different(key1, key2, "KDF_sensitivity_to_salt")
    
    def test_encryption_decryption(self):
        """Test encryption/decryption round-trip."""
        enc = DeterministicEncryption(self.config)
        plaintext = b"This is a test message"
        password = b"test_password"
        
        encrypted = enc.encrypt(plaintext, password)
        decrypted = enc.decrypt(encrypted, password)
        
        self.assert_equal(decrypted, plaintext, "encryption_decryption_roundtrip")
    
    def test_deterministic_encryption(self):
        """Test deterministic encryption produces same ciphertext."""
        enc = DeterministicEncryption(self.config)
        plaintext = b"test data"
        password = b"password"
        
        enc1 = enc.encrypt_deterministic(plaintext, password)
        enc2 = enc.encrypt_deterministic(plaintext, password)
        
        self.assert_equal(enc1, enc2, "deterministic_encryption")
    
    def test_wrong_password_fails(self):
        """Test decryption with wrong password fails."""
        enc = DeterministicEncryption(self.config)
        plaintext = b"secret"
        
        encrypted = enc.encrypt(plaintext, b"correct_password")
        
        try:
            enc.decrypt(encrypted, b"wrong_password")
            self.assert_true(False, "wrong_password_authentication")
        except ValueError:
            self.assert_true(True, "wrong_password_authentication")
    
    def test_mac_determinism(self):
        """Test MAC is deterministic."""
        mac_gen = DeterministicMAC(self.config)
        message = b"test message"
        key = b"mac_key"
        
        mac1 = mac_gen.compute_mac(message, key)
        mac2 = mac_gen.compute_mac(message, key)
        
        self.assert_equal(mac1, mac2, "MAC_determinism")
    
    def test_mac_verification(self):
        """Test MAC verification."""
        mac_gen = DeterministicMAC(self.config)
        message = b"authenticated message"
        key = b"secret_key"
        
        mac = mac_gen.compute_mac(message, key)
        verified = mac_gen.verify_mac(message, mac, key)
        
        self.assert_true(verified, "MAC_verification")
    
    def test_mac_tampering_detection(self):
        """Test MAC detects message tampering."""
        mac_gen = DeterministicMAC(self.config)
        message = b"original message"
        key = b"secret_key"
        
        mac = mac_gen.compute_mac(message, key)
        modified = b"modified message"
        
        # MAC should fail for modified message
        verified = mac_gen.verify_mac(modified, mac, key)
        self.assert_true(not verified, "MAC_tampering_detection")
    
    def test_commitment_reveal(self):
        """Test commitment and reveal."""
        commitment_gen = CryptographicCommitment(self.config)
        data = b"secret_data"
        
        comm_id, randomness = commitment_gen.commit(data)
        verified = commitment_gen.reveal(comm_id, data, randomness)
        
        self.assert_true(verified, "commitment_reveal")
    
    def test_commitment_binding(self):
        """Test commitment is binding (can't forge different data)."""
        commitment_gen = CryptographicCommitment(self.config)
        data = b"original_data"
        
        comm_id, randomness = commitment_gen.commit(data)
        
        # Try to reveal with different data
        verified_wrong = commitment_gen.reveal(comm_id, data + b"X", randomness)
        
        self.assert_true(not verified_wrong, "commitment_binding")
    
    def test_message_sequencing(self):
        """Test message sequence tracking."""
        seq = SecureMessageSequence(self.config)
        
        msg1 = seq.create_message(MessageType.ENCRYPTED, b"first")
        msg2 = seq.create_message(MessageType.ENCRYPTED, b"second")
        msg3 = seq.create_message(MessageType.ENCRYPTED, b"third")
        
        self.assert_true(msg1.sequence < msg2.sequence, "message_seq_1")
        self.assert_true(msg2.sequence < msg3.sequence, "message_seq_2")
    
    def test_crypto_chain(self):
        """Test cryptographic chain operations."""
        chain = CryptographicChain(self.config)
        
        chain.add_operation("op1", b"data1")
        chain.add_operation("op2", b"data2")
        
        proof = chain.get_chain_proof()
        
        self.assert_equal(proof["operation_count"], 2, "chain_operation_count")
        self.assert_true(isinstance(proof["chain_hash"], str), "chain_has_hash")
    
    def test_multiparty_exchange(self):
        """Test multi-party key exchange."""
        mpke1 = MultiPartyKeyExchange(self.config)
        mpke2 = MultiPartyKeyExchange(self.config)
        
        contributions = [b"party1", b"party2", b"party3"]
        salt = b"shared_salt_1234"
        
        for contrib in contributions:
            mpke1.add_contribution(contrib)
            mpke2.add_contribution(contrib)
        
        key1 = mpke1.derive_shared_key(salt)
        key2 = mpke2.derive_shared_key(salt)
        
        self.assert_equal(key1, key2, "multiparty_exchange_determinism")
    
    def test_entropy_analysis(self):
        """Test entropy analysis."""
        analyzer = ProtocolAnalyzer()
        
        # Random data should have high entropy
        random_data = hashlib.sha256(b"test").digest()
        entropy = analyzer.analyze_entropy(random_data)
        
        self.assert_true(entropy["entropy"] > 3.0, "entropy_analysis_random")
    
    def test_output_comparison(self):
        """Test output comparison for avalanche effect."""
        analyzer = ProtocolAnalyzer()
        
        data1 = hashlib.sha256(b"test1").digest()
        data2 = hashlib.sha256(b"test2").digest()
        
        comparison = analyzer.compare_outputs(data1, data2)
        
        self.assert_true(not comparison["identical"], "output_comparison")
    
    def test_rhash_avalanche(self):
        """Test in-folding rhash avalanche effect."""
        rhash = InFoldingRhash(self.config)
        salt = b"test_salt_16byte"
        
        data1 = b"test data 1"
        data2 = b"test data 2"
        
        hash1 = rhash.refactorize(data1, salt)
        hash2 = rhash.refactorize(data2, salt)
        
        self.assert_different(hash1, hash2, "rhash_avalanche_effect")
    
    def test_performance_kdf(self):
        """Measure KDF performance."""
        kdf = DeterministicKDF(self.config)
        password = b"password"
        salt = b"salt_16_bytes12"
        
        start = time.time()
        kdf.derive_key(password, salt, 32)
        elapsed = time.time() - start
        
        # Should complete in reasonable time (< 5 seconds for 10k iterations)
        self.assert_true(elapsed < 5, f"KDF_performance ({elapsed:.2f}s)")
    
    def test_performance_encryption(self):
        """Measure encryption performance."""
        enc = DeterministicEncryption(self.config)
        plaintext = b"x" * 10000  # 10KB
        password = b"password"
        
        start = time.time()
        enc.encrypt(plaintext, password)
        elapsed = time.time() - start
        
        # Should complete quickly
        self.assert_true(elapsed < 2, f"encryption_performance ({elapsed:.2f}s)")
    
    def run_all_tests(self) -> Dict[str, Any]:
        """Run all tests."""
        test_methods = [
            self.test_kdf_determinism,
            self.test_kdf_sensitivity,
            self.test_kdf_salt_sensitivity,
            self.test_encryption_decryption,
            self.test_deterministic_encryption,
            self.test_wrong_password_fails,
            self.test_mac_determinism,
            self.test_mac_verification,
            self.test_mac_tampering_detection,
            self.test_commitment_reveal,
            self.test_commitment_binding,
            self.test_message_sequencing,
            self.test_crypto_chain,
            self.test_multiparty_exchange,
            self.test_entropy_analysis,
            self.test_output_comparison,
            self.test_rhash_avalanche,
            self.test_performance_kdf,
            self.test_performance_encryption,
        ]
        
        for test_method in test_methods:
            try:
                test_method()
            except Exception as e:
                test_name = test_method.__name__
                self.failed += 1
                self._record_test(test_name, False, str(e))
        
        return self.report()


def run_benchmark():
    """Run performance benchmark."""
    print("=== Cryptographic Protocol Benchmarks ===\n")
    
    config = EncryptionConfig(
        hash_algo=HashAlgorithm.SHA256,
        fold_depth=5,
        kdf_iterations=100000
    )
    
    # KDF benchmark
    print("1. KDF Benchmark:")
    kdf = DeterministicKDF(config)
    password = b"benchmark_password"
    salt = b"benchmark_salt16"
    
    start = time.time()
    key = kdf.derive_key(password, salt, 32)
    elapsed = time.time() - start
    print(f"   100,000 iterations: {elapsed:.3f} seconds")
    
    # Encryption benchmark
    print("\n2. Encryption Benchmark:")
    enc = DeterministicEncryption(config)
    
    for size_kb in [1, 10, 100]:
        plaintext = b"x" * (size_kb * 1024)
        start = time.time()
        encrypted = enc.encrypt(plaintext, password)
        elapsed = time.time() - start
        throughput = size_kb / elapsed if elapsed > 0 else 0
        print(f"   {size_kb}KB: {elapsed:.3f}s ({throughput:.1f} MB/s)")
    
    # Rhash benchmark
    print("\n3. In-Folding Rhash Benchmark:")
    rhash = InFoldingRhash(config)
    
    for fold_depth in [1, 3, 5, 10]:
        config_bench = EncryptionConfig(fold_depth=fold_depth)
        rhash_bench = InFoldingRhash(config_bench)
        
        data = b"x" * 1000
        start = time.time()
        rhash_bench.refactorize(data)
        elapsed = time.time() - start
        print(f"   Depth {fold_depth}: {elapsed * 1000:.2f}ms")


if __name__ == "__main__":
    # Run tests
    print("=== Cryptographic Protocol Test Suite ===\n")
    
    suite = CryptoTestSuite()
    report = suite.run_all_tests()
    
    # Print report
    print(f"Tests Passed: {report['passed']}/{report['total']} ({report['pass_rate']:.1f}%)\n")
    
    if report['failed'] > 0:
        print("Failed tests:")
        for test_name, result in report['results'].items():
            if not result['passed']:
                print(f"  ✗ {test_name}: {result['error']}")
    else:
        print("✓ All tests passed!")
    
    print("\n" + "="*50 + "\n")
    
    # Run benchmarks
    run_benchmark()
    
    print("\n" + "="*50)
    print("Test and benchmark suite completed.")
