"""
Quick Start Guide: Deterministic Encryption + Graphics Shell

Fast-track guide to get started with the cryptographic protocol and visualization.
"""

import sys
from pathlib import Path

# ============================================================================
# PART 1: SIMPLE ENCRYPTION/DECRYPTION
# ============================================================================

def quickstart_basic_encryption():
    """Simplest encryption example."""
    from deterministic_crypto import DeterministicEncryption, EncryptionConfig
    
    print("=== Basic Encryption ===\n")
    
    # Setup
    config = EncryptionConfig()
    enc = DeterministicEncryption(config)
    
    plaintext = b"Hello, World!"
    password = b"my_password"
    
    # Encrypt
    encrypted = enc.encrypt(plaintext, password)
    print(f"Plaintext:  {plaintext}")
    print(f"Encrypted:  {encrypted[:32]}... (truncated)")
    
    # Decrypt
    decrypted = enc.decrypt(encrypted, password)
    print(f"Decrypted:  {decrypted}")
    print(f"Match: {plaintext == decrypted}\n")


# ============================================================================
# PART 2: DETERMINISTIC ENCRYPTION
# ============================================================================

def quickstart_deterministic_mode():
    """Deterministic encryption (same output for same input)."""
    from deterministic_crypto import DeterministicEncryption, EncryptionConfig
    
    print("=== Deterministic Encryption ===\n")
    
    config = EncryptionConfig()
    enc = DeterministicEncryption(config)
    
    plaintext = b"Fixed data"
    password = b"fixed_password"
    
    # Encrypt twice - should be identical
    enc1 = enc.encrypt_deterministic(plaintext, password)
    enc2 = enc.encrypt_deterministic(plaintext, password)
    
    print(f"Encryption 1: {enc1[:32].hex()}...")
    print(f"Encryption 2: {enc2[:32].hex()}...")
    print(f"Identical: {enc1 == enc2}\n")


# ============================================================================
# PART 3: KEY DERIVATION
# ============================================================================

def quickstart_key_derivation():
    """Derive cryptographic keys from passwords."""
    from deterministic_crypto import DeterministicKDF, EncryptionConfig
    
    print("=== Key Derivation (KDF) ===\n")
    
    config = EncryptionConfig(kdf_iterations=50000)
    kdf = DeterministicKDF(config)
    
    password = b"user_password"
    salt = b"random_salt_1234"
    
    # Derive 32-byte key
    key = kdf.derive_key(password, salt, 32)
    
    print(f"Password: {password}")
    print(f"Salt:     {salt}")
    print(f"Key:      {key.hex()[:32]}...")
    print(f"Key length: {len(key)} bytes\n")


# ============================================================================
# PART 4: MESSAGE AUTHENTICATION
# ============================================================================

def quickstart_message_authentication():
    """Authenticate messages with MAC."""
    from crypto_protocols import DeterministicMAC
    from deterministic_crypto import EncryptionConfig
    
    print("=== Message Authentication Code (MAC) ===\n")
    
    config = EncryptionConfig()
    mac_gen = DeterministicMAC(config)
    
    message = b"Important message"
    key = b"secret_key"
    
    # Compute MAC
    mac = mac_gen.compute_mac(message, key)
    print(f"Message: {message}")
    print(f"MAC:     {mac.hex()[:32]}...")
    
    # Verify
    verified = mac_gen.verify_mac(message, mac, key)
    print(f"Verified: {verified}\n")


# ============================================================================
# PART 5: VISUALIZATION WITH GRAPHICS SHELL
# ============================================================================

def quickstart_visualization():
    """Visualize encryption operation."""
    from graphics_shell import ResultType, GraphicsShellManager
    from deterministic_crypto import DeterministicEncryption, EncryptionConfig
    
    print("=== Visualization with Graphics Shell ===\n")
    
    from PySide6.QtWidgets import QApplication
    
    app = QApplication.instance() or QApplication(sys.argv)
    manager = GraphicsShellManager()
    
    # Create simple encryption stats
    config = EncryptionConfig()
    enc = DeterministicEncryption(config)
    
    plaintext = b"x" * 1000
    password = b"password"
    encrypted = enc.encrypt(plaintext, password)
    
    # Display results
    stats = {
        "Encryption Statistics": {
            "Plaintext Size": f"{len(plaintext)} bytes",
            "Ciphertext Size": f"{len(encrypted)} bytes",
            "Expansion Ratio": f"{len(encrypted) / len(plaintext):.2f}x",
            "Hash Algorithm": config.hash_algo.value,
            "Fold Depth": config.fold_depth,
            "KDF Iterations": config.kdf_iterations,
        }
    }
    
    manager.show_result(ResultType.TREE, stats, title="Encryption Stats")
    
    print("Window opened. Close to continue.\n")
    sys.exit(app.exec())


# ============================================================================
# PART 6: COMMAND-LINE USAGE
# ============================================================================

def quickstart_cli_examples():
    """Show CLI usage examples."""
    print("=== Command-Line Interface Examples ===\n")
    
    examples = [
        ("Encrypt text", 
         'python crypto_cli.py encrypt --password "secret" --data "message"'),
        
        ("Encrypt file",
         'python crypto_cli.py encrypt --password "secret" --file plaintext.txt'),
        
        ("Deterministic encryption",
         'python crypto_cli.py encrypt --password "secret" --data "test" --deterministic'),
        
        ("Decrypt",
         'python crypto_cli.py decrypt --password "secret" --data "<encrypted_base64>"'),
        
        ("Derive key",
         'python crypto_cli.py kdf --password "secret" --salt "1234567890123456" --length 32'),
        
        ("Compute MAC",
         'python crypto_cli.py mac --message "data" --key "secret_key"'),
        
        ("Analyze entropy",
         'python crypto_cli.py analyze entropy --data "test_data"'),
    ]
    
    for title, cmd in examples:
        print(f"{title}:")
        print(f"  {cmd}\n")


# ============================================================================
# PART 7: COMPLETE WORKFLOW
# ============================================================================

def quickstart_complete_workflow():
    """Complete encryption workflow."""
    from deterministic_crypto import (
        DeterministicEncryption, DeterministicKDF, EncryptionConfig
    )
    from crypto_protocols import DeterministicMAC
    
    print("=== Complete Encryption Workflow ===\n")
    
    config = EncryptionConfig()
    
    # 1. Derive key
    print("1. Deriving encryption key...")
    kdf = DeterministicKDF(config)
    password = b"user_password"
    salt = b"random_salt_1234"
    key = kdf.derive_key(password, salt, 32)
    print(f"   ✓ Key derived: {key.hex()[:16]}...\n")
    
    # 2. Encrypt data
    print("2. Encrypting data...")
    enc = DeterministicEncryption(config)
    plaintext = b"Sensitive information"
    encrypted = enc.encrypt(plaintext, password)
    print(f"   ✓ Encrypted: {len(encrypted)} bytes\n")
    
    # 3. Compute authentication tag
    print("3. Computing authentication code...")
    mac_gen = DeterministicMAC(config)
    mac = mac_gen.compute_mac(encrypted, key)
    print(f"   ✓ MAC: {mac.hex()[:16]}...\n")
    
    # 4. Decrypt and verify
    print("4. Decrypting and verifying...")
    decrypted = enc.decrypt(encrypted, password)
    verified = mac_gen.verify_mac(encrypted, mac, key)
    print(f"   ✓ Decrypted: {decrypted}")
    print(f"   ✓ Verified: {verified}\n")
    
    print("Workflow complete!\n")


# ============================================================================
# MAIN MENU
# ============================================================================

def main():
    """Main menu."""
    examples = [
        ("Basic Encryption/Decryption", quickstart_basic_encryption),
        ("Deterministic Mode", quickstart_deterministic_mode),
        ("Key Derivation", quickstart_key_derivation),
        ("Message Authentication", quickstart_message_authentication),
        ("Graphics Shell Visualization", quickstart_visualization),
        ("CLI Examples", quickstart_cli_examples),
        ("Complete Workflow", quickstart_complete_workflow),
    ]
    
    print("╔════════════════════════════════════════════════════════════╗")
    print("║  Deterministic Encryption Protocol - Quick Start Guide    ║")
    print("╚════════════════════════════════════════════════════════════╝\n")
    
    print("Available examples:\n")
    for i, (name, _) in enumerate(examples, 1):
        print(f"  {i}. {name}")
    
    print(f"  {len(examples) + 1}. Run all examples")
    print(f"  {len(examples) + 2}. Exit\n")
    
    while True:
        try:
            choice = input("Select example (number): ").strip()
            choice_num = int(choice)
            
            if choice_num == len(examples) + 1:
                # Run all
                for name, func in examples:
                    print("\n" + "="*60)
                    print(f"Example: {name}")
                    print("="*60 + "\n")
                    try:
                        func()
                    except Exception as e:
                        print(f"Error: {e}\n")
                break
            elif choice_num == len(examples) + 2:
                print("Goodbye!")
                break
            elif 1 <= choice_num <= len(examples):
                name, func = examples[choice_num - 1]
                print("\n" + "="*60)
                print(f"Example: {name}")
                print("="*60 + "\n")
                func()
                break
            else:
                print("Invalid choice. Try again.\n")
        except ValueError:
            print("Please enter a number.\n")
        except KeyboardInterrupt:
            print("\nInterrupted. Goodbye!")
            break
        except Exception as e:
            print(f"Error: {e}\n")


if __name__ == "__main__":
    main()
