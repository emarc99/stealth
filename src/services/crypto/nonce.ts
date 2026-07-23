/**
 * Algorithm-aware nonce helpers (#1694).
 *
 * Centralizes secure nonce generation and strict validation so that every
 * crypto path uses a consistent nonce length, encoding, and uniqueness story.
 *
 * Production always uses `crypto.getRandomValues`. A deterministic generator
 * can be injected for tests only; it must never be used in production paths.
 */

import { CryptoError, cryptoFail, cryptoOk, type CryptoResult } from "./errors";

/** Algorithm suites and the nonce length (in bytes) each expects. */
export const NONCE_LENGTHS = {
  "AES-256-GCM": 12,
  "AES-128-GCM": 12,
  "ChaCha20-Poly1305": 12,
} as const;

export type NonceAlgorithm = keyof typeof NONCE_LENGTHS;

/** A test-only deterministic generator; undefined means use production CSPRNG. */
export type NonceGenerator = (length: number) => Uint8Array;

let injectedGenerator: NonceGenerator | undefined;

/**
 * Test seam: install a deterministic generator. Passing `undefined` restores
 * production behavior. MUST NOT be used outside tests.
 */
export function __setNonceGeneratorForTesting(generator: NonceGenerator | undefined): void {
  injectedGenerator = generator;
}

function randomBytes(length: number): Uint8Array {
  if (injectedGenerator) {
    return injectedGenerator(length);
  }
  if (typeof crypto === "undefined" || typeof crypto.getRandomValues !== "function") {
    throw new CryptoError("crypto_algorithm_error", "secure random source unavailable");
  }
  return crypto.getRandomValues(new Uint8Array(length));
}

/** Generate a fresh nonce for the given algorithm suite. */
export function generateNonce(algorithm: NonceAlgorithm): Uint8Array {
  const length = NONCE_LENGTHS[algorithm];
  const nonce = randomBytes(length);
  if (nonce.length !== length) {
    throw new CryptoError("crypto_algorithm_error", "nonce generation length mismatch");
  }
  return nonce;
}

const HEX_REGEX = /^[0-9a-fA-F]*$/;

/**
 * Decode a hex nonce string into bytes, validating that the alphabet is strict
 * hex and the resulting length matches the algorithm's expected nonce length.
 * Returns a typed result so callers branch on `ok` without parsing messages.
 */
export function decodeNonce(
  value: string,
  algorithm: NonceAlgorithm,
): CryptoResult<Uint8Array> {
  if (typeof value !== "string" || value.length === 0) {
    return cryptoFail(new CryptoError("crypto_validation_error", "nonce must be a non-empty string"));
  }
  if (value.length % 2 !== 0) {
    return cryptoFail(new CryptoError("crypto_validation_error", "nonce hex length must be even"));
  }
  if (!HEX_REGEX.test(value)) {
    return cryptoFail(new CryptoError("crypto_validation_error", "nonce contains non-hex characters"));
  }

  const bytes = new Uint8Array(value.length / 2);
  for (let i = 0; i < bytes.length; i += 1) {
    bytes[i] = Number.parseInt(value.slice(i * 2, i * 2 + 2), 16);
  }

  const expected = NONCE_LENGTHS[algorithm];
  if (bytes.length !== expected) {
    return cryptoFail(
      new CryptoError("crypto_validation_error", `nonce must be ${expected} bytes for ${algorithm}`),
    );
  }

  return cryptoOk(bytes);
}

/** Encode a nonce byte array to canonical lowercase hex. */
export function encodeNonce(nonce: Uint8Array): string {
  let out = "";
  for (const b of nonce) {
    out += b.toString(16).padStart(2, "0");
  }
  return out;
}

/**
 * Verify a decoded nonce is exactly the algorithm's expected length. Useful when
 * a nonce arrives already as bytes (e.g. from a parsed structure) so it fails
 * before any crypto call.
 */
export function validateNonceLength(
  nonce: Uint8Array,
  algorithm: NonceAlgorithm,
): CryptoResult<Uint8Array> {
  const expected = NONCE_LENGTHS[algorithm];
  if (nonce.length !== expected) {
    return cryptoFail(
      new CryptoError("crypto_validation_error", `nonce must be ${expected} bytes for ${algorithm}`),
    );
  }
  return cryptoOk(nonce);
}
