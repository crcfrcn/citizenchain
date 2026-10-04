// BLAKE2b-256 PoW mining kernel for CitizenChain.
//
// Each work-item computes blake2_256(pre_hash || nonce) where
//   nonce = nonce_base + get_global_id(0)
// and checks whether the resulting hash meets the difficulty target.
//
// Algorithm: BLAKE2b (RFC 7693), truncated to 256-bit output.

// ---------- BLAKE2b constants ----------

// Initialization vector (first 8 words of the fractional parts of sqrt of first 8 primes).
__constant ulong IV[8] = {
    0x6a09e667f3bcc908UL, 0xbb67ae8584caa73bUL,
    0x3c6ef372fe94f82bUL, 0xa54ff53a5f1d36f1UL,
    0x510e527fade682d1UL, 0x9b05688c2b3e6c1fUL,
    0x1f83d9abfb41bd6bUL, 0x5be0cd19137e2179UL
};

// Sigma permutation table (10 rounds for BLAKE2b).
__constant uchar SIGMA[10][16] = {
    { 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,14,15},
    {14,10, 4, 8, 9,15,13, 6, 1,12, 0, 2,11, 7, 5, 3},
    {11, 8,12, 0, 5, 2,15,13,10,14, 3, 6, 7, 1, 9, 4},
    { 7, 9, 3, 1,13,12,11,14, 2, 6, 5,10, 4, 0,15, 8},
    { 9, 0, 5, 7, 2, 4,10,15,14, 1,11,12, 6, 8, 3,13},
    { 2,12, 6,10, 0,11, 8, 3, 4,13, 7, 5,15,14, 1, 9},
    {12, 5, 1,15,14,13, 4,10, 0, 7, 6, 3, 9, 2, 8,11},
    {13,11, 7,14,12, 1, 3, 9, 5, 0,15, 4, 8, 6, 2,10},
    { 6,15,14, 9,11, 3, 0, 8,12, 2,13, 7, 1, 4,10, 5},
    {10, 2, 8, 4, 7, 6, 1, 5,15,11, 9,14, 3,12,13, 0}
};

// ---------- BLAKE2b mixing ----------

#define ROTR64(x, n) (((x) >> (n)) | ((x) << (64 - (n))))

#define G(a, b, c, d, x, y) do { \
    a = a + b + x;               \
    d = ROTR64(d ^ a, 32);       \
    c = c + d;                    \
    b = ROTR64(b ^ c, 24);       \
    a = a + b + y;               \
    d = ROTR64(d ^ a, 16);       \
    c = c + d;                    \
    b = ROTR64(b ^ c, 63);       \
} while(0)

// ---------- BLAKE2b-256 single-block hash ----------
// Computes blake2b-256 of a message that fits in one block (0..128 bytes).
// For PoW: message = pre_hash(32 bytes) + nonce(8 bytes) = 40 bytes.

static void blake2b_256_oneblock(
    const ulong *msg_words,  // message padded to 16 x ulong (little-endian)
    uint msg_len,            // actual message length in bytes
    ulong *out               // 4 x ulong output (256-bit hash)
) {
    // State initialization.
    // h[0] = IV[0] ^ 0x01010020  (fanout=1, depth=1, digest_length=32)
    ulong h[8];
    h[0] = IV[0] ^ 0x01010020UL;
    h[1] = IV[1];
    h[2] = IV[2];
    h[3] = IV[3];
    h[4] = IV[4];
    h[5] = IV[5];
    h[6] = IV[6];
    h[7] = IV[7];

    // Compress (single block, final).
    ulong v[16];
    v[0] = h[0]; v[1] = h[1]; v[2] = h[2]; v[3] = h[3];
    v[4] = h[4]; v[5] = h[5]; v[6] = h[6]; v[7] = h[7];
    v[8]  = IV[0]; v[9]  = IV[1]; v[10] = IV[2]; v[11] = IV[3];
    v[12] = IV[4] ^ (ulong)msg_len;  // t0 = msg_len (counter low)
    v[13] = IV[5];                     // t1 = 0 (counter high)
    v[14] = ~IV[6];                    // f0 inverted = final block flag
    v[15] = IV[7];

    // 12 rounds of mixing.
    for (int r = 0; r < 12; r++) {
        int s = r % 10;
        G(v[0], v[4], v[ 8], v[12], msg_words[SIGMA[s][ 0]], msg_words[SIGMA[s][ 1]]);
        G(v[1], v[5], v[ 9], v[13], msg_words[SIGMA[s][ 2]], msg_words[SIGMA[s][ 3]]);
        G(v[2], v[6], v[10], v[14], msg_words[SIGMA[s][ 4]], msg_words[SIGMA[s][ 5]]);
        G(v[3], v[7], v[11], v[15], msg_words[SIGMA[s][ 6]], msg_words[SIGMA[s][ 7]]);
        G(v[0], v[5], v[10], v[15], msg_words[SIGMA[s][ 8]], msg_words[SIGMA[s][ 9]]);
        G(v[1], v[6], v[11], v[12], msg_words[SIGMA[s][10]], msg_words[SIGMA[s][11]]);
        G(v[2], v[7], v[ 8], v[13], msg_words[SIGMA[s][12]], msg_words[SIGMA[s][13]]);
        G(v[3], v[4], v[ 9], v[14], msg_words[SIGMA[s][14]], msg_words[SIGMA[s][15]]);
    }

    // Finalize: h[i] ^= v[i] ^ v[i+8], output first 4 words (256 bits).
    out[0] = h[0] ^ v[0] ^ v[ 8];
    out[1] = h[1] ^ v[1] ^ v[ 9];
    out[2] = h[2] ^ v[2] ^ v[10];
    out[3] = h[3] ^ v[3] ^ v[11];
}

// ---------- Difficulty comparison ----------
// target[4] and hash[4] are big-endian ulong arrays (word 0 = most significant).
// Returns true if hash <= target.
static bool hash_le_target(const ulong *hash_be, __global const ulong *target_be) {
    for (int i = 0; i < 4; i++) {
        if (hash_be[i] < target_be[i]) return true;
        if (hash_be[i] > target_be[i]) return false;
    }
    return true; // equal
}

// ---------- Byte-swap for big-endian conversion ----------
static ulong bswap64(ulong x) {
    x = ((x & 0x00000000FFFFFFFFUL) << 32) | ((x & 0xFFFFFFFF00000000UL) >> 32);
    x = ((x & 0x0000FFFF0000FFFFUL) << 16) | ((x & 0xFFFF0000FFFF0000UL) >> 16);
    x = ((x & 0x00FF00FF00FF00FFUL) <<  8) | ((x & 0xFF00FF00FF00FF00UL) >>  8);
    return x;
}

// ---------- Main mining kernel ----------
//
// Parameters:
//   pre_hash:    32 bytes (the block pre-hash)
//   nonce_base:  starting nonce; actual nonce = nonce_base + get_global_id(0)
//   target:      4 x ulong, big-endian (U256::MAX / difficulty)
//   result_nonce: output buffer for the found nonce
//   found:       atomic flag, set to 1 when a valid nonce is found
//
__kernel void blake2b_pow_mine(
    __global const uchar *pre_hash,
    ulong nonce_base,
    __global const ulong *target,
    __global ulong *result_nonce,
    __global volatile uint *found
) {
    // Early exit if another work-item already found a solution.
    if (*found != 0) return;

    ulong nonce = nonce_base + (ulong)get_global_id(0);

    // Build the 40-byte message: pre_hash(32) || nonce(8, little-endian).
    // Pad to 128 bytes (16 x ulong) with zeros for BLAKE2b single-block compress.
    ulong msg[16];
    // Zero-initialize all 16 words.
    for (int i = 0; i < 16; i++) msg[i] = 0;

    // Copy pre_hash (32 bytes = 4 x ulong, little-endian word order).
    __global const ulong *ph64 = (__global const ulong *)pre_hash;
    msg[0] = ph64[0];
    msg[1] = ph64[1];
    msg[2] = ph64[2];
    msg[3] = ph64[3];
    // Nonce in word 4 (bytes 32..39), little-endian.
    msg[4] = nonce;
    // Words 5..15 remain zero (padding).

    // Compute BLAKE2b-256.
    ulong hash[4];
    blake2b_256_oneblock(msg, 40, hash);

    // Convert hash from little-endian words to big-endian for comparison.
    // Substrate's U256::from_big_endian interprets hash bytes in big-endian order.
    // BLAKE2b outputs little-endian words, so we need to:
    //   1. Byte-swap each word
    //   2. Reverse word order (word 3 becomes most significant)
    ulong hash_be[4];
    hash_be[0] = bswap64(hash[3]);
    hash_be[1] = bswap64(hash[2]);
    hash_be[2] = bswap64(hash[1]);
    hash_be[3] = bswap64(hash[0]);

    if (hash_le_target(hash_be, target)) {
        // Atomically set the found flag; only the first finder writes result.
        uint old = atomic_cmpxchg((__global volatile uint *)found, 0, 1);
        if (old == 0) {
            *result_nonce = nonce;
        }
    }
}
