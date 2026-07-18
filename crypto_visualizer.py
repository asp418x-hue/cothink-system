"""
Cryptographic Visualization and Graphics Shell Integration

Visualize cryptographic operations, key derivation, and protocol
analysis using the PySide6 graphics shell.
"""

import json
import hashlib
from typing import Dict, Any, List
from dataclasses import asdict

from graphics_shell import ResultType, GraphicsShellManager, ResultConfig
from graphics_shell_utils import ShellPipeline, CustomResultRenderer
from deterministic_crypto import (
    DeterministicEncryption, DeterministicKDF, InFoldingRhash,
    EncryptionConfig, analyze_encryption
)
from crypto_protocols import (
    DeterministicMAC, CryptographicChain, CryptographicCommitment,
    ProtocolAnalyzer, MultiPartyKeyExchange
)


class CryptoVisualizer:
    """
    Visualize cryptographic operations using graphics shell.
    """
    
    def __init__(self, manager: GraphicsShellManager):
        self.manager = manager
        self.analyzer = ProtocolAnalyzer()
    
    def visualize_encryption(self, plaintext: bytes, password: bytes,
                            config: EncryptionConfig) -> str:
        """Visualize encryption process and statistics."""
        enc = DeterministicEncryption(config)
        encrypted = enc.encrypt(plaintext, password)
        
        stats = analyze_encryption(plaintext, encrypted, config)
        
        viz_data = {
            "Operation": "Deterministic Encryption",
            "Plaintext Size": f"{stats.plaintext_size} bytes",
            "Ciphertext Size": f"{stats.ciphertext_size} bytes",
            "Expansion Ratio": f"{stats.expansion_ratio:.2f}x",
            "Hash Algorithm": stats.hash_algo,
            "Fold Depth": stats.fold_depth,
            "KDF Iterations": stats.kdf_iterations,
            "Plaintext Hash": hashlib.sha256(plaintext).hexdigest()[:16],
            "Ciphertext Hash": hashlib.sha256(encrypted).hexdigest()[:16],
        }
        
        return self.manager.show_result(
            ResultType.TREE,
            viz_data,
            title="Encryption Analysis",
            width=800,
            height=600
        )
    
    def visualize_kdf(self, password: bytes, salt: bytes,
                     config: EncryptionConfig) -> str:
        """Visualize KDF process and key derivation."""
        kdf = DeterministicKDF(config)
        
        # Derive keys at different iterations
        iterations = [1000, 10000, 50000, 100000]
        key_hashes = {}
        
        for it in iterations:
            key = kdf.derive_key(password, salt, 32, it)
            key_hashes[f"Iterations {it}"] = hashlib.sha256(key).hexdigest()[:16]
        
        # Check consistency
        key1 = kdf.derive_key(password, salt, 32)
        key2 = kdf.derive_key(password, salt, 32)
        
        viz_data = {
            "KDF Analysis": {
                "Hash Algorithm": config.hash_algo.value,
                "Fold Depth": config.fold_depth,
                "Default Iterations": config.kdf_iterations,
                "Key Derivations": key_hashes,
                "Consistency": {
                    "Key1 Hash": hashlib.sha256(key1).hexdigest()[:16],
                    "Key2 Hash": hashlib.sha256(key2).hexdigest()[:16],
                    "Deterministic": key1 == key2
                }
            }
        }
        
        return self.manager.show_result(
            ResultType.TREE,
            viz_data,
            title="KDF Analysis",
            width=800,
            height=600
        )
    
    def visualize_rhash_folding(self, data: bytes,
                               config: EncryptionConfig) -> str:
        """Visualize in-folding rhash refactorization."""
        rhash = InFoldingRhash(config)
        
        # Track hash at each fold
        salt = hashlib.sha256(b"visualization_salt").digest()[:config.salt_size]
        fold_hashes = []
        
        # Simulate folding process
        current_hash = hashlib.sha256(data).digest()
        for fold_idx in range(min(config.fold_depth, 10)):
            current_hash = rhash._fold_iteration(current_hash, data, fold_idx)
            fold_hashes.append(hashlib.sha256(current_hash).hexdigest()[:16])
        
        # Final refactorized hash
        final_hash = rhash.refactorize(data, salt)
        
        viz_data = {
            "In-Folding Rhash": {
                "Configuration": {
                    "Fold Depth": config.fold_depth,
                    "Hash Algorithm": config.hash_algo.value,
                },
                "Folding Progression": {
                    f"Fold {i}": hash_val 
                    for i, hash_val in enumerate(fold_hashes)
                },
                "Final Refactorized Hash": hashlib.sha256(final_hash).hexdigest()[:16],
                "Input Data Hash": hashlib.sha256(data).hexdigest()[:16],
            }
        }
        
        return self.manager.show_result(
            ResultType.TREE,
            viz_data,
            title="In-Folding Rhash Visualization",
            width=900,
            height=700
        )
    
    def visualize_mac_verification(self, message: bytes, key: bytes,
                                   config: EncryptionConfig) -> str:
        """Visualize MAC computation and verification."""
        mac_gen = DeterministicMAC(config)
        mac = mac_gen.compute_mac(message, key)
        
        # Compute another MAC with same inputs
        mac2 = mac_gen.compute_mac(message, key)
        
        # Try with modified message
        modified_msg = bytes([message[0] ^ 1 if message else 0]) + message[1:]
        mac_modified = mac_gen.compute_mac(modified_msg, key)
        
        viz_data = {
            "Message Authentication Code": {
                "Original Message": {
                    "Length": len(message),
                    "Hash": hashlib.sha256(message).hexdigest()[:16],
                },
                "MAC Generation": {
                    "MAC 1 Hash": hashlib.sha256(mac).hexdigest()[:16],
                    "MAC 2 Hash": hashlib.sha256(mac2).hexdigest()[:16],
                    "Deterministic": mac == mac2,
                },
                "Verification": {
                    "Original Verified": mac_gen.verify_mac(message, mac, key),
                    "Modified Verified": mac_gen.verify_mac(modified_msg, mac_modified, key),
                    "Cross-verify (should fail)": mac_gen.verify_mac(modified_msg, mac, key),
                },
                "Modified Message": {
                    "Length": len(modified_msg),
                    "Hash": hashlib.sha256(modified_msg).hexdigest()[:16],
                }
            }
        }
        
        return self.manager.show_result(
            ResultType.TREE,
            viz_data,
            title="MAC Verification",
            width=900,
            height=700
        )
    
    def visualize_crypto_chain(self, operations: List[tuple],
                              config: EncryptionConfig) -> str:
        """Visualize cryptographic chain operations."""
        chain = CryptographicChain(config)
        
        # Add operations
        for op_name, data in operations:
            chain.add_operation(op_name, data)
        
        proof = chain.get_chain_proof()
        
        return self.manager.show_result(
            ResultType.TREE,
            proof,
            title="Cryptographic Chain",
            width=900,
            height=600
        )
    
    def visualize_commitment(self, data: bytes,
                           config: EncryptionConfig) -> str:
        """Visualize commitment scheme."""
        commitment_gen = CryptographicCommitment(config)
        comm_id, randomness = commitment_gen.commit(data)
        verified = commitment_gen.reveal(comm_id, data, randomness)
        
        # Try invalid reveal
        invalid_verified = commitment_gen.reveal(comm_id, data + b"x", randomness)
        
        viz_data = {
            "Commitment Scheme": {
                "Data": {
                    "Size": len(data),
                    "Hash": hashlib.sha256(data).hexdigest()[:16],
                },
                "Commitment": {
                    "ID": comm_id,
                    "Randomness Hash": hashlib.sha256(randomness).hexdigest()[:16],
                },
                "Verification": {
                    "Valid Reveal": verified,
                    "Invalid Reveal": invalid_verified,
                },
                "Configuration": {
                    "Hash Algorithm": config.hash_algo.value,
                    "Fold Depth": config.fold_depth,
                }
            }
        }
        
        return self.manager.show_result(
            ResultType.TREE,
            viz_data,
            title="Commitment Scheme",
            width=800,
            height=600
        )
    
    def visualize_entropy_analysis(self, data: bytes) -> str:
        """Visualize entropy analysis."""
        entropy_info = self.analyzer.analyze_entropy(data)
        
        # Create visualization-friendly format
        viz_data = {
            "Data Analysis": {
                "Total Bytes": entropy_info['total_bytes'],
                "Unique Bytes": entropy_info['unique_bytes'],
                "Entropy": f"{entropy_info['entropy']:.4f}",
                "Entropy Bits": f"{entropy_info['entropy_bits']:.2f}",
                "Byte Count Distribution": {
                    f"Byte {byte}": count
                    for byte, count in list(entropy_info['distribution'].items())[:16]
                }
            }
        }
        
        return self.manager.show_result(
            ResultType.TREE,
            viz_data,
            title="Entropy Analysis",
            width=800,
            height=600
        )
    
    def visualize_output_comparison(self, output1: bytes, output2: bytes) -> str:
        """Compare two cryptographic outputs."""
        comparison = self.analyzer.compare_outputs(output1, output2)
        
        return self.manager.show_result(
            ResultType.TREE,
            comparison,
            title="Output Comparison",
            width=700,
            height=500
        )
    
    def visualize_multi_party_exchange(self, contributions: List[bytes],
                                     shared_salt: bytes,
                                     config: EncryptionConfig) -> str:
        """Visualize multi-party key exchange."""
        mpke = MultiPartyKeyExchange(config)
        
        for contribution in contributions:
            mpke.add_contribution(contribution)
        
        shared_key = mpke.derive_shared_key(shared_salt)
        
        viz_data = {
            "Multi-Party Key Exchange": {
                "Configuration": {
                    "Participants": len(contributions),
                    "Hash Algorithm": config.hash_algo.value,
                    "Fold Depth": config.fold_depth,
                    "KDF Iterations": config.kdf_iterations,
                },
                "Contributions": {
                    f"Party {i+1}": hashlib.sha256(contrib).hexdigest()[:16]
                    for i, contrib in enumerate(contributions)
                },
                "Shared Salt Hash": hashlib.sha256(shared_salt).hexdigest()[:16],
                "Derived Shared Key": {
                    "Length": len(shared_key),
                    "Hash": hashlib.sha256(shared_key).hexdigest()[:16],
                }
            }
        }
        
        return self.manager.show_result(
            ResultType.TREE,
            viz_data,
            title="Multi-Party Key Exchange",
            width=900,
            height=600
        )


def run_comprehensive_demo():
    """Run comprehensive cryptographic visualization demo."""
    from PySide6.QtWidgets import QApplication
    import sys
    
    app = QApplication.instance() or QApplication(sys.argv)
    manager = GraphicsShellManager()
    visualizer = CryptoVisualizer(manager)
    
    # Configuration
    config = EncryptionConfig(
        fold_depth=5,
        kdf_iterations=50000
    )
    
    # Test data
    plaintext = b"The quick brown fox jumps over the lazy dog"
    password = b"secure_password_123"
    message = b"Message to authenticate"
    key = b"secret_key"
    
    print("Launching cryptographic visualization demo...\n")
    
    # 1. Encryption visualization
    visualizer.visualize_encryption(plaintext, password, config)
    
    # 2. KDF visualization
    salt = hashlib.sha256(b"test_salt").digest()[:16]
    visualizer.visualize_kdf(password, salt, config)
    
    # 3. In-folding rhash visualization
    visualizer.visualize_rhash_folding(plaintext, config)
    
    # 4. MAC verification
    visualizer.visualize_mac_verification(message, key, config)
    
    # 5. Cryptographic chain
    operations = [
        ("hash", plaintext),
        ("compress", plaintext[:10]),
        ("expand", plaintext + plaintext),
    ]
    visualizer.visualize_crypto_chain(operations, config)
    
    # 6. Commitment
    visualizer.visualize_commitment(plaintext, config)
    
    # 7. Entropy analysis
    random_data = hashlib.sha256(plaintext).digest()
    visualizer.visualize_entropy_analysis(random_data)
    
    # 8. Multi-party exchange
    contributions = [
        b"party_1_contribution",
        b"party_2_contribution",
        b"party_3_contribution",
    ]
    shared_salt = b"shared_salt_12345"
    visualizer.visualize_multi_party_exchange(contributions, shared_salt, config)
    
    print("All visualizations displayed.")
    print("Close windows to exit.")
    
    sys.exit(app.exec())


if __name__ == "__main__":
    run_comprehensive_demo()
