# Deterministic Encryption Protocol with In-Folding Rhash

A comprehensive cryptographic protocol featuring deterministic encryption, in-folding recursive hashing (rhash), and advanced cryptographic primitives.

## Overview

### Key Components

1. **In-Folding Rhash Refactorization** - Recursive, multi-layered hashing where each fold incorporates:
   - Previous hash state (creates dependency chain)
   - Current data segment
   - Fold iteration number
   - Refactorization constants

2. **Deterministic Key Derivation Function (KDF)** - Derives keys from passwords using in-folding rhash with configurable iterations

3. **Deterministic Encryption** - AES-256-CTR with:
   - Deterministic IV generation from plaintext
   - HMAC authentication
   - Optional true deterministic mode (fixed salt)

4. **Advanced Cryptographic Protocols**:
   - Message sequencing with replay protection
   - Commitment schemes
   - Zero-knowledge proofs (simplified)
   - Deterministic MAC
   - Multi-party key exchange
   - Cryptographic chains

## Architecture

### In-Folding Rhash (Rhash)

The rhash refactorization algorithm performs recursive hashing with the following characteristics:

```
Initial Hash: H₀ = hash(salt || data)

For each fold i from 1 to depth:
  - Extract data segment Sᵢ
  - Compute refactorization constant Cᵢ
  - Perform nested hash of H_{i-1}
  - Combine: H_i = hash(H_{i-1} || S_i || i || C_i || nested_hash)
  
Final output: H_depth
```

**Properties:**
- Deterministic: Same input always produces same output
- Avalanche effect: Changing one bit affects all subsequent folds
- Dependency chain: Each fold depends on all previous operations
- Configurable depth for different security/performance tradeoffs

### Deterministic KDF

```
For iteration j from 1 to iterations:
  H_j = rhash(H_{j-1} + iteration_counter, salt)
  
For key expansion if needed:
  K = H_iterations || rhash(H_iterations, salt, counter=1) || ...
  
Return K[:key_length]
```

### Deterministic Encryption

```
Format: [version:1 byte][salt:16 bytes][ciphertext:n bytes][hmac:32 bytes]

Encrypt:
  1. Derive cipher_key, auth_key = KDF(password, salt)
  2. Generate IV = rhash(plaintext || "IV_GEN", salt)[:16]
  3. ciphertext = AES-256-CTR(plaintext, cipher_key, IV)
  4. auth_tag = HMAC-SHA256(version || salt || ciphertext, auth_key)
  5. Return version || salt || ciphertext || auth_tag

Decrypt:
  1. Verify version and extract salt
  2. Derive cipher_key, auth_key = KDF(password, salt)
  3. Verify HMAC
  4. Generate IV same as encrypt
  5. plaintext = AES-256-CTR-decrypt(ciphertext, cipher_key, IV)
```

## Files

### Core Cryptography
- `deterministic_crypto.py` - Encryption, KDF, in-folding rhash
- `crypto_protocols.py` - Advanced protocols and utilities
- `crypto_visualizer.py` - Graphics shell visualization
- `crypto_cli.py` - Command-line interface

## Usage

### Python API

#### Basic Encryption

```python
from deterministic_crypto import DeterministicEncryption, EncryptionConfig

config = EncryptionConfig(fold_depth=5, kdf_iterations=100000)
enc = DeterministicEncryption(config)

plaintext = b"Secret message"
password = b"secure_password"

# Encrypt (random salt - different ciphertext each time)
encrypted = enc.encrypt(plaintext, password)

# Decrypt
decrypted = enc.decrypt(encrypted, password)
assert decrypted == plaintext
```

#### True Deterministic Encryption

```python
# Same plaintext + password = same ciphertext
det1 = enc.encrypt_deterministic(plaintext, password)
det2 = enc.encrypt_deterministic(plaintext, password)
assert det1 == det2  # True!
```

#### Key Derivation

```python
from deterministic_crypto import DeterministicKDF

kdf = DeterministicKDF(config)
salt = b"sixteen_byte_sal"

key = kdf.derive_key(password, salt, key_length=32, iterations=100000)

# Verify determinism
key2 = kdf.derive_key(password, salt, key_length=32, iterations=100000)
assert key == key2  # True!
```

#### Message Authentication

```python
from crypto_protocols import DeterministicMAC

mac_gen = DeterministicMAC(config)
message = b"Data to authenticate"
key = b"secret_key"

mac = mac_gen.compute_mac(message, key)

# Verify
if mac_gen.verify_mac(message, mac, key):
    print("Authentication passed")
```

#### Commitment Scheme

```python
from crypto_protocols import CryptographicCommitment

commitment_gen = CryptographicCommitment(config)
data = b"secret_value"

# Commit to value
comm_id, randomness = commitment_gen.commit(data)

# Later, reveal without exposing data initially
if commitment_gen.reveal(comm_id, data, randomness):
    print("Commitment verified")
```

#### Cryptographic Chain

```python
from crypto_protocols import CryptographicChain

chain = CryptographicChain(config)

chain.add_operation("hash", b"initial_data")
chain.add_operation("encrypt", b"more_data")
chain.add_operation("sign", b"final_data")

proof = chain.get_chain_proof()
print(proof["chain_hash"])
```

#### Multi-Party Key Exchange

```python
from crypto_protocols import MultiPartyKeyExchange

# Each party
mpke = MultiPartyKeyExchange(config)
mpke.add_contribution(b"party_1_contribution")
mpke.add_contribution(b"party_2_contribution")
mpke.add_contribution(b"party_3_contribution")

shared_key = mpke.derive_shared_key(b"shared_salt_12345")
```

### Command-Line Interface

```bash
# Make executable
chmod +x crypto_cli.py

# Encrypt text
./crypto_cli.py encrypt --password "secret" --data "message"

# Encrypt file
./crypto_cli.py encrypt --password "secret" --file data.txt

# Deterministic encryption (same output for same input)
./crypto_cli.py encrypt --password "secret" --data "test" --deterministic

# Decrypt
./crypto_cli.py decrypt --password "secret" --data "<encrypted_base64>"

# Key derivation
./crypto_cli.py kdf --password "secret" --salt "1234567890123456" --length 32

# Compute MAC
./crypto_cli.py mac --message "data" --key "secret"

# Create commitment
./crypto_cli.py commit --data "secret"

# Analyze entropy
./crypto_cli.py analyze entropy --data "test_data"

# Compare outputs
./crypto_cli.py analyze compare --output1 "deadbeef..." --output2 "cafebabe..."
```

### Graphics Shell Visualization

```python
from crypto_visualizer import CryptoVisualizer, GraphicsShellManager
from deterministic_crypto import EncryptionConfig

manager = GraphicsShellManager()
visualizer = CryptoVisualizer(manager)
config = EncryptionConfig()

# Visualize encryption process
visualizer.visualize_encryption(b"plaintext", b"password", config)

# Visualize KDF derivation
visualizer.visualize_kdf(b"password", b"salt_16_bytes", config)

# Visualize in-folding rhash
visualizer.visualize_rhash_folding(b"data", config)

# Visualize MAC operations
visualizer.visualize_mac_verification(b"message", b"key", config)

# Visualize cryptographic chains
operations = [("hash", b"data1"), ("encrypt", b"data2")]
visualizer.visualize_crypto_chain(operations, config)

# Visualize commitments
visualizer.visualize_commitment(b"secret", config)

# Analyze entropy
visualizer.visualize_entropy_analysis(b"random_data")

# Multi-party exchange
visualizer.visualize_multi_party_exchange(
    [b"party1", b"party2", b"party3"],
    b"salt_16_bytes",
    config
)
```

## Configuration

### EncryptionConfig Parameters

```python
@dataclass
class EncryptionConfig:
    hash_algo: HashAlgorithm = HashAlgorithm.SHA256  # SHA256/SHA512/SHA3-256/SHA3-512
    fold_depth: int = 5                             # In-folding recursion depth
    kdf_iterations: int = 100000                    # KDF iteration count
    cipher_key_size: int = 32                       # 256-bit for AES-256
    auth_key_size: int = 32                         # HMAC-SHA256 key size
    salt_size: int = 16                             # 128-bit salt
    version: int = 1                                # Protocol version
```

### Security Recommendations

- **KDF iterations**: At least 100,000 for passwords. Use more for high-security scenarios.
- **Fold depth**: 5-10 for balance of security and performance
- **Hash algorithm**: SHA-256 for most uses; SHA-512 for additional security
- **Salt**: Always use random salt unless deterministic output is required

## Security Properties

### Deterministic Encryption

✅ **Deterministic**: Same plaintext + key produces same ciphertext (when using fixed salt)  
✅ **Authenticated**: HMAC prevents tampering  
✅ **Semantic security** (with random salt): Different ciphertexts for multiple encryptions  
⚠️ **Note**: Deterministic encryption reveals pattern information; use only when necessary

### In-Folding Rhash

✅ **Deterministic**: Repeatable for same inputs  
✅ **Avalanche effect**: Single bit change affects all subsequent folds  
✅ **Dependency chain**: Each fold depends on all previous operations  
✅ **Slow by design**: Configurable depth prevents brute force

### KDF

✅ **Deterministic**: Always derives same key from same password/salt  
✅ **Slow**: Thousands of iterations make brute force expensive  
✅ **Expandable**: Can derive keys of arbitrary length

## Performance

### Typical Performance (on modern CPU)

- **KDF** (100,000 iterations): ~100-500ms
- **Encryption** (1MB): ~10-50ms
- **In-folding rhash** (depth 5): ~5-20ms per 1KB

### Optimization Tips

- Reduce `kdf_iterations` for non-security-critical uses
- Reduce `fold_depth` for performance-critical operations
- Use SHA-256 instead of SHA-512 for better performance
- Cache derived keys when possible

## Security Considerations

### Deterministic vs. Semantic Security

**Deterministic encryption** (fixed salt):
- Pros: Repeatable, queryable database fields, deduplication
- Cons: Reveals plaintext patterns, vulnerable to chosen-plaintext attacks

**Semantic encryption** (random salt):
- Pros: Hides plaintext patterns, stronger security
- Cons: Same plaintext produces different ciphertexts

Use deterministic encryption only when required for application logic.

### Side-Channel Resistance

- HMAC verification uses `hmac.compare_digest()` for constant-time comparison
- In-folding rhash design resists timing attacks through uniform iteration count
- Consider additional measures for high-security scenarios

## Testing

```bash
# Run basic tests
python deterministic_crypto.py

# Run advanced protocol tests
python crypto_protocols.py

# Run visualization demo
python crypto_visualizer.py

# CLI tests
python crypto_cli.py encrypt --password test --data "hello" --deterministic
python crypto_cli.py analyze entropy --data "test"
```

## Examples

### Secure Password Storage

```python
# Generate unique salt per user
import os
user_salt = os.urandom(16)

# Derive storage key
kdf = DeterministicKDF(config)
storage_key = kdf.derive_key(user_password, user_salt, 32)

# Store: user_salt (plaintext) + storage_key (hash or use as secret key)
```

### Encrypted Database Fields

```python
# Deterministic encryption for searchable encrypted database
plaintext = "user@example.com"
password = "column_encryption_key"

# Always produces same ciphertext for same email
encrypted_email = enc.encrypt_deterministic(plaintext.encode(), password.encode())

# Can search database: WHERE encrypted_email = <encrypted_value>
```

### Multi-Party Secure Computation

```python
# Three parties derive shared encryption key
mpke = MultiPartyKeyExchange(config)
mpke.add_contribution(party1_secret)
mpke.add_contribution(party2_secret)
mpke.add_contribution(party3_secret)

shared_key = mpke.derive_shared_key(public_salt)
# Use shared_key for symmetric encryption
```

## References

- AES: FIPS 197
- HMAC: RFC 2104
- SHA: FIPS 180-4
- Cryptography.io: https://cryptography.io/

## License

MIT - Use and modify freely.

## Future Enhancements

- Post-quantum resistant variants
- Hardware acceleration support
- Streaming encryption for large files
- Real zero-knowledge proof implementations
- Distributed key exchange protocols
