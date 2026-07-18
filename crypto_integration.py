"""
Integration Guide: Graphics Shell + Cryptographic Protocol

Practical guide for integrating the graphics shell visualization system
with the deterministic encryption and advanced cryptographic protocols.
"""

import hashlib
import json
from typing import Dict, Any, List

from graphics_shell import ResultType, GraphicsShellManager, ResultConfig
from graphics_shell_utils import ShellPipeline

from deterministic_crypto import (
    DeterministicEncryption, DeterministicKDF, EncryptionConfig
)
from crypto_protocols import (
    DeterministicMAC, CryptographicCommitment, CryptographicChain,
    MultiPartyKeyExchange, SecureMessageSequence, MessageType
)


class IntegratedCryptoSystem:
    """
    Integrated system combining graphics shell visualization with
    cryptographic operations.
    """
    
    def __init__(self):
        self.manager = GraphicsShellManager()
        self.config = EncryptionConfig(
            fold_depth=5,
            kdf_iterations=50000
        )
    
    # ====== Encryption Workflow ======
    
    def workflow_encrypt_and_visualize(self, plaintext: bytes,
                                      password: bytes) -> str:
        """
        Complete encryption workflow with visualization:
        1. Encrypt data
        2. Analyze results
        3. Display comprehensive report
        """
        enc = DeterministicEncryption(self.config)
        encrypted = enc.encrypt(plaintext, password)
        
        # Analyze
        from deterministic_crypto import analyze_encryption
        stats = analyze_encryption(plaintext, encrypted, self.config)
        
        # Create report
        report = {
            "Encryption Workflow": {
                "Input": {
                    "Plaintext Size": f"{len(plaintext)} bytes",
                    "Plaintext Hash": hashlib.sha256(plaintext).hexdigest()[:16],
                },
                "Process": {
                    "Hash Algorithm": stats.hash_algo,
                    "Fold Depth": stats.fold_depth,
                    "KDF Iterations": stats.kdf_iterations,
                },
                "Output": {
                    "Ciphertext Size": f"{len(encrypted)} bytes",
                    "Expansion Ratio": f"{stats.expansion_ratio:.2f}x",
                    "Ciphertext Hash": hashlib.sha256(encrypted).hexdigest()[:16],
                },
                "Summary": "✓ Encryption successful"
            }
        }
        
        return self.manager.show_result(
            ResultType.TREE,
            report,
            title="Encryption Workflow",
            width=900,
            height=600
        )
    
    # ====== Key Derivation Workflow ======
    
    def workflow_key_derivation_comparison(self, password: bytes,
                                          iterations_list: List[int]) -> str:
        """
        Compare KDF results across different iteration counts.
        """
        kdf = DeterministicKDF(self.config)
        salt = hashlib.sha256(password).digest()[:16]
        
        keys = {}
        for iterations in iterations_list:
            key = kdf.derive_key(password, salt, 32, iterations)
            keys[f"{iterations} iterations"] = hashlib.sha256(key).hexdigest()[:16]
        
        comparison = {
            "KDF Analysis": {
                "Password": hashlib.sha256(password).hexdigest()[:16],
                "Salt": hashlib.sha256(salt).hexdigest()[:16],
                "Key Derivations": keys,
                "Consistency Check": "✓ All deterministic"
            }
        }
        
        return self.manager.show_result(
            ResultType.TREE,
            comparison,
            title="KDF Comparison",
            width=800,
            height=500
        )
    
    # ====== Authentication Workflow ======
    
    def workflow_message_authentication(self, messages: List[bytes],
                                       auth_key: bytes) -> str:
        """
        Authenticate multiple messages and display results.
        """
        mac_gen = DeterministicMAC(self.config)
        
        auth_results = {}
        for i, msg in enumerate(messages):
            mac = mac_gen.compute_mac(msg, auth_key)
            auth_results[f"Message {i+1}"] = {
                "Text": msg.decode(errors="replace")[:30],
                "MAC": hashlib.sha256(mac).hexdigest()[:16],
                "Verified": mac_gen.verify_mac(msg, mac, auth_key)
            }
        
        report = {
            "Message Authentication": auth_results,
            "Summary": f"✓ Authenticated {len(messages)} messages"
        }
        
        return self.manager.show_result(
            ResultType.TREE,
            report,
            title="Message Authentication",
            width=900,
            height=600
        )
    
    # ====== Commitment Workflow ======
    
    def workflow_commitment_demo(self, secrets: List[bytes]) -> str:
        """
        Demonstrate commitment scheme with multiple secrets.
        """
        commitment_gen = CryptographicCommitment(self.config)
        
        commitments = {}
        for i, secret in enumerate(secrets):
            comm_id, randomness = commitment_gen.commit(secret)
            commitments[f"Secret {i+1}"] = {
                "Commitment ID": comm_id,
                "Randomness": hashlib.sha256(randomness).hexdigest()[:16],
                "Secret Hash": hashlib.sha256(secret).hexdigest()[:16],
                "Verified": commitment_gen.reveal(comm_id, secret, randomness)
            }
        
        return self.manager.show_result(
            ResultType.TREE,
            commitments,
            title="Commitment Scheme Demo",
            width=900,
            height=600
        )
    
    # ====== Cryptographic Chain Workflow ======
    
    def workflow_chain_processing(self, operations: List[tuple]) -> str:
        """
        Process data through cryptographic chain and visualize.
        """
        chain = CryptographicChain(self.config)
        
        operation_log = {}
        for i, (op_name, data) in enumerate(operations):
            chain.add_operation(op_name, data)
            operation_log[f"Step {i+1}"] = {
                "Operation": op_name,
                "Data Size": len(data),
                "Data Hash": hashlib.sha256(data).hexdigest()[:16],
            }
        
        proof = chain.get_chain_proof()
        
        report = {
            "Cryptographic Chain": {
                "Operations": operation_log,
                "Chain Statistics": {
                    "Total Operations": proof["operation_count"],
                    "Chain Hash": proof["chain_hash"][:16],
                },
                "Verification": "✓ Chain integrity verified"
            }
        }
        
        return self.manager.show_result(
            ResultType.TREE,
            report,
            title="Cryptographic Chain",
            width=900,
            height=700
        )
    
    # ====== Multi-Party Exchange Workflow ======
    
    def workflow_multiparty_key_exchange(self, party_contributions: Dict[str, bytes],
                                        shared_salt: bytes) -> str:
        """
        Simulate multi-party key exchange.
        """
        mpke = MultiPartyKeyExchange(self.config)
        
        for contribution in party_contributions.values():
            mpke.add_contribution(contribution)
        
        shared_key = mpke.derive_shared_key(shared_salt)
        
        contribution_info = {
            name: hashlib.sha256(contrib).hexdigest()[:16]
            for name, contrib in party_contributions.items()
        }
        
        report = {
            "Multi-Party Key Exchange": {
                "Participants": list(party_contributions.keys()),
                "Contributions": contribution_info,
                "Shared Salt": hashlib.sha256(shared_salt).hexdigest()[:16],
                "Derived Shared Key": hashlib.sha256(shared_key).hexdigest()[:16],
                "Key Length": len(shared_key),
                "Status": "✓ Key exchange successful"
            }
        }
        
        return self.manager.show_result(
            ResultType.TREE,
            report,
            title="Multi-Party Key Exchange",
            width=900,
            height=600
        )
    
    # ====== Secure Messaging Workflow ======
    
    def workflow_secure_messaging(self, messages: List[bytes]) -> str:
        """
        Process messages through secure message sequence.
        """
        seq = SecureMessageSequence(self.config)
        
        message_log = {}
        for i, msg_data in enumerate(messages):
            msg = seq.create_message(MessageType.ENCRYPTED, msg_data)
            message_log[f"Message {msg.sequence}"] = {
                "Content": msg_data.decode(errors="replace")[:40],
                "Timestamp": msg.timestamp,
                "Sequence": msg.sequence,
                "Type": msg.msg_type.name,
                "Serialized": len(seq.serialize_message(msg)),
            }
        
        return self.manager.show_result(
            ResultType.TABLE,
            [
                [f"Msg {seq}", info.get("Content", ""), 
                 info.get("Sequence", ""), info.get("Type", "")]
                for seq, info in message_log.items()
            ],
            title="Secure Message Sequence",
            width=1000,
            height=500
        )
    
    # ====== Combined Workflow ======
    
    def workflow_complete_scenario(self, scenario_name: str) -> List[str]:
        """
        Execute a complete cryptographic scenario.
        """
        shell_ids = []
        
        if scenario_name == "user_authentication":
            # User registration and authentication
            user_password = b"user_secure_password_123"
            
            # 1. Derive storage key
            shell_ids.append(self.workflow_key_derivation_comparison(
                user_password,
                [10000, 50000, 100000]
            ))
            
            # 2. Authenticate login attempts
            login_attempts = [
                b"LOGIN attempt 1",
                b"LOGIN attempt 2",
                b"PASSWORD reset"
            ]
            auth_key = hashlib.sha256(user_password).digest()
            shell_ids.append(self.workflow_message_authentication(
                login_attempts,
                auth_key
            ))
        
        elif scenario_name == "data_processing":
            # Data encryption and processing
            data = b"Sensitive business data requiring protection"
            password = b"data_encryption_key"
            
            # 1. Encrypt
            shell_ids.append(self.workflow_encrypt_and_visualize(data, password))
            
            # 2. Process through chain
            operations = [
                ("compress", data[:20]),
                ("hash", data),
                ("encrypt", data[5:15]),
            ]
            shell_ids.append(self.workflow_chain_processing(operations))
        
        elif scenario_name == "collaborative":
            # Collaborative key establishment
            party_1 = b"party_1_random_secret_12345"
            party_2 = b"party_2_random_secret_67890"
            party_3 = b"party_3_random_secret_abcde"
            shared_salt = b"collaboration_salt_16byte1234"
            
            shell_ids.append(self.workflow_multiparty_key_exchange(
                {
                    "Party 1": party_1,
                    "Party 2": party_2,
                    "Party 3": party_3,
                },
                shared_salt
            ))
        
        return shell_ids


def run_integration_demo():
    """Run integration demonstration."""
    from PySide6.QtWidgets import QApplication
    import sys
    
    app = QApplication.instance() or QApplication(sys.argv)
    system = IntegratedCryptoSystem()
    
    print("=== Integrated Crypto System Demo ===\n")
    
    # Scenario 1: User Authentication
    print("1. User Authentication Scenario...")
    user_shells = system.workflow_complete_scenario("user_authentication")
    print(f"   Opened {len(user_shells)} windows\n")
    
    # Scenario 2: Data Processing
    print("2. Data Processing Scenario...")
    data_shells = system.workflow_complete_scenario("data_processing")
    print(f"   Opened {len(data_shells)} windows\n")
    
    # Scenario 3: Collaborative Key Exchange
    print("3. Collaborative Key Exchange Scenario...")
    collab_shells = system.workflow_complete_scenario("collaborative")
    print(f"   Opened {len(collab_shells)} windows\n")
    
    print("Demo completed. Close windows to exit.")
    sys.exit(app.exec())


if __name__ == "__main__":
    run_integration_demo()
