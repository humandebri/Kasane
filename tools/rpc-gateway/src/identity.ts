// どこで: Gateway identity解決層 / 何を: PEMから署名identityを復元 / なぜ: update callをanonymousにしないため

import type { SignIdentity } from "@dfinity/agent";
import { createPrivateKey } from "node:crypto";
import { Ed25519KeyIdentity } from "@dfinity/identity";
import { Secp256k1KeyIdentity } from "@dfinity/identity-secp256k1";

export function identityFromPem(pem: string): SignIdentity {
  try {
    return Secp256k1KeyIdentity.fromPem(pem);
  } catch (secpError) {
    try {
      return identityFromEd25519Pem(pem);
    } catch (edError) {
      const secpMessage = secpError instanceof Error ? secpError.message : String(secpError);
      const edMessage = edError instanceof Error ? edError.message : String(edError);
      throw new Error(`Unsupported identity PEM (secp256k1: ${secpMessage}; ed25519: ${edMessage})`);
    }
  }
}

function identityFromEd25519Pem(pem: string): Ed25519KeyIdentity {
  const keyObject = createPrivateKey(pem);
  if (keyObject.asymmetricKeyType !== "ed25519") {
    throw new Error(`unexpected key type: ${keyObject.asymmetricKeyType ?? "unknown"}`);
  }
  const jwk = keyObject.export({ format: "jwk" });
  const secretD = jwk.d;
  if (typeof secretD !== "string") {
    throw new Error("missing JWK 'd' field");
  }
  const secretKey = decodeBase64Url(secretD);
  if (secretKey.length !== 32) {
    throw new Error(`invalid ed25519 secret length: ${secretKey.length}`);
  }
  return Ed25519KeyIdentity.fromSecretKey(secretKey);
}

function decodeBase64Url(value: string): Uint8Array {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, "=");
  return Uint8Array.from(Buffer.from(padded, "base64"));
}
