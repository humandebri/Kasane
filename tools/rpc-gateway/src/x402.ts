// where: x402 facilitator route layer / what: verifies and settles exact EVM payments / why: allow Kasane ERC-3009 tokens to serve x402 resources

import { Interface, JsonRpcProvider, Wallet } from "ethers";
import { CONFIG } from "./config.js";
import { parseJsonWithDepthLimit } from "./jsonrpc.js";
import type { RpcHttpResponse } from "./http.js";

const EXACT_SCHEME = "exact";
const X402_VERSION = 2;
const RECEIVE_WITH_AUTHORIZATION = "receiveWithAuthorization";
const AUTHORIZATION_STATE = "authorizationState";

const TOKEN = new Interface([
  "function name() view returns (string)",
  "function version() view returns (string)",
  "function authorizationState(address authorizer, bytes32 nonce) view returns (bool)",
  "function receiveWithAuthorization(address from,address to,uint256 value,uint256 validAfter,uint256 validBefore,bytes32 nonce,uint8 v,bytes32 r,bytes32 s)",
]);

export type X402HttpRequest = {
  method: string;
  path: string;
  readBodyText: () => Promise<string>;
};

type PaymentRequirements = {
  scheme: string;
  network: string;
  asset: string;
  amount: string;
  payTo: string;
  maxTimeoutSeconds: number;
  extra: {
    name: string;
    version: string;
  };
};

type Authorization = {
  from: string;
  to: string;
  value: string;
  validAfter: string;
  validBefore: string;
  nonce: string;
};

type PaymentPayload = {
  x402Version: number;
  accepted: PaymentRequirements;
  payload: {
    signature: string;
    authorization: Authorization;
  };
};

type FacilitatorBody = {
  x402Version: number;
  paymentPayload: PaymentPayload;
  paymentRequirements: PaymentRequirements;
};

type VerifyResponse = {
  isValid: boolean;
  payer: string;
  invalidReason?: string;
  invalidMessage?: string;
  extra: Record<string, string>;
};

type SettleResponse = {
  success: boolean;
  payer: string;
  transaction: string;
  network: string;
  errorReason?: string;
  errorMessage?: string;
  amount: string;
  extra: Record<string, string>;
};

export async function handleX402Http(input: X402HttpRequest): Promise<RpcHttpResponse> {
  if (input.method === "OPTIONS") {
    return jsonResponse(204, null);
  }
  if (input.path === "/v2/x402/supported" && input.method === "GET") {
    return jsonResponse(200, {
      kinds: [{ x402Version: X402_VERSION, scheme: EXACT_SCHEME, network: CONFIG.x402Network }],
      extensions: {},
    });
  }
  if (input.method !== "POST") {
    return jsonResponse(405, { error: "method not allowed" });
  }
  if (input.path !== "/v2/x402/verify" && input.path !== "/v2/x402/settle") {
    return jsonResponse(404, { error: "not found" });
  }

  let body: FacilitatorBody;
  try {
    body = parseFacilitatorBody(parseJsonWithDepthLimit(await input.readBodyText(), CONFIG.maxJsonDepth));
  } catch (err) {
    return jsonResponse(400, {
      error: err instanceof Error ? err.message : String(err),
    });
  }

  if (input.path === "/v2/x402/verify") {
    return jsonResponse(200, await verifyPayment(body));
  }
  return jsonResponse(200, await settlePayment(body));
}

export async function verifyPayment(body: FacilitatorBody): Promise<VerifyResponse> {
  const shapeError = validateBodyShape(body);
  if (shapeError !== null) {
    return invalid(body.paymentPayload.payload.authorization.from, shapeError, shapeError);
  }

  const provider = new JsonRpcProvider(CONFIG.x402RpcUrl, Number(CONFIG.x402Network.slice("eip155:".length)), {
    staticNetwork: true,
  });
  const req = body.paymentRequirements;
  const auth = body.paymentPayload.payload.authorization;
  try {
    if ((await tokenString(provider, req.asset, "name")) !== req.extra.name) {
      return invalid(auth.from, "invalid_exact_evm_token_name_mismatch", "token name mismatch");
    }
    if ((await tokenString(provider, req.asset, "version")) !== req.extra.version) {
      return invalid(auth.from, "invalid_exact_evm_token_version_mismatch", "token version mismatch");
    }
    if (await authorizationUsed(provider, req.asset, auth.from, auth.nonce)) {
      return invalid(auth.from, "invalid_exact_evm_nonce_already_used", "authorization nonce already used");
    }
    await provider.call({
      from: req.payTo,
      to: req.asset,
      data: receiveWithAuthorizationCalldata(body.paymentPayload),
    });
    return { isValid: true, payer: auth.from, extra: {} };
  } catch (err) {
    return invalid(auth.from, "invalid_exact_evm_verification_failed", errMessage(err));
  }
}

export async function settlePayment(body: FacilitatorBody): Promise<SettleResponse> {
  const auth = body.paymentPayload.payload.authorization;
  const shapeError = validateBodyShape(body);
  if (shapeError !== null) {
    return {
      success: false,
      payer: auth.from,
      transaction: zeroTxHash(),
      network: CONFIG.x402Network,
      errorReason: shapeError,
      errorMessage: shapeError,
      amount: body.paymentRequirements.amount,
      extra: {},
    };
  }
  if (CONFIG.x402SettlerPrivateKey === null) {
    return {
      success: false,
      payer: auth.from,
      transaction: zeroTxHash(),
      network: CONFIG.x402Network,
      errorReason: "settle_exact_node_failure",
      errorMessage: "X402_SETTLER_PRIVATE_KEY is required",
      amount: body.paymentRequirements.amount,
      extra: {},
    };
  }
  let wallet: Wallet;
  try {
    wallet = new Wallet(CONFIG.x402SettlerPrivateKey);
  } catch (err) {
    return {
      success: false,
      payer: auth.from,
      transaction: zeroTxHash(),
      network: CONFIG.x402Network,
      errorReason: "settle_exact_node_failure",
      errorMessage: errMessage(err),
      amount: body.paymentRequirements.amount,
      extra: {},
    };
  }
  if (!sameLower(wallet.address, body.paymentRequirements.payTo)) {
    return {
      success: false,
      payer: auth.from,
      transaction: zeroTxHash(),
      network: CONFIG.x402Network,
      errorReason: "settle_exact_evm_pay_to_mismatch",
      errorMessage: "X402_SETTLER_PRIVATE_KEY address must match paymentRequirements.payTo",
      amount: body.paymentRequirements.amount,
      extra: {},
    };
  }

  const verified = await verifyPayment(body);
  if (!verified.isValid) {
    return {
      success: false,
      payer: auth.from,
      transaction: zeroTxHash(),
      network: CONFIG.x402Network,
      errorReason: verified.invalidReason,
      errorMessage: verified.invalidMessage,
      amount: body.paymentRequirements.amount,
      extra: {},
    };
  }

  try {
    const provider = new JsonRpcProvider(CONFIG.x402RpcUrl, Number(CONFIG.x402Network.slice("eip155:".length)), {
      staticNetwork: true,
    });
    const connectedWallet = wallet.connect(provider);
    const tx = await connectedWallet.sendTransaction({
      to: body.paymentRequirements.asset,
      data: receiveWithAuthorizationCalldata(body.paymentPayload),
      value: 0,
    });
    return {
      success: true,
      payer: auth.from,
      transaction: tx.hash,
      network: CONFIG.x402Network,
      amount: body.paymentRequirements.amount,
      extra: {},
    };
  } catch (err) {
    return {
      success: false,
      payer: auth.from,
      transaction: zeroTxHash(),
      network: CONFIG.x402Network,
      errorReason: "invalid_exact_evm_failed_to_execute_transfer",
      errorMessage: errMessage(err),
      amount: body.paymentRequirements.amount,
      extra: {},
    };
  }
}

export function receiveWithAuthorizationCalldata(payment: PaymentPayload): string {
  const auth = payment.payload.authorization;
  const sig = splitSignature(payment.payload.signature);
  return TOKEN.encodeFunctionData(RECEIVE_WITH_AUTHORIZATION, [
    auth.from,
    auth.to,
    auth.value,
    auth.validAfter,
    auth.validBefore,
    auth.nonce,
    sig.v,
    sig.r,
    sig.s,
  ]);
}

function validateBodyShape(body: FacilitatorBody): string | null {
  const req = body.paymentRequirements;
  const accepted = body.paymentPayload.accepted;
  const auth = body.paymentPayload.payload.authorization;
  if (body.x402Version !== X402_VERSION || body.paymentPayload.x402Version !== X402_VERSION) {
    return "invalid_x402_version";
  }
  if (req.scheme !== EXACT_SCHEME || accepted.scheme !== EXACT_SCHEME) {
    return "invalid_exact_evm_scheme";
  }
  if (req.network !== CONFIG.x402Network || accepted.network !== req.network) {
    return "invalid_exact_evm_network_mismatch";
  }
  if (!sameLower(req.asset, accepted.asset)) {
    return "invalid_batch_settlement_evm_token_mismatch";
  }
  if (req.amount !== accepted.amount || req.amount !== auth.value) {
    return "invalid_exact_evm_payload_authorization_value_mismatch";
  }
  if (!sameLower(req.payTo, accepted.payTo) || !sameLower(req.payTo, auth.to)) {
    return "invalid_exact_evm_recipient_mismatch";
  }
  if (!isHexAddress(req.asset) || !isHexAddress(req.payTo) || !isHexAddress(auth.from) || !isHexAddress(auth.to)) {
    return "invalid_payload";
  }
  if (!isUintText(req.amount) || !isUintText(auth.value) || !isUintText(auth.validAfter) || !isUintText(auth.validBefore)) {
    return "invalid_payload";
  }
  if (!/^0x[0-9a-fA-F]{64}$/.test(auth.nonce)) {
    return "invalid_exact_evm_payload";
  }
  if (!/^0x[0-9a-fA-F]{130}$/.test(body.paymentPayload.payload.signature)) {
    return "invalid_exact_evm_signature_format";
  }
  return null;
}

async function tokenString(provider: JsonRpcProvider, token: string, method: "name" | "version"): Promise<string> {
  const data = TOKEN.encodeFunctionData(method, []);
  const result = await provider.call({ to: token, data });
  const decoded = TOKEN.decodeFunctionResult(method, result);
  return String(decoded[0]);
}

async function authorizationUsed(
  provider: JsonRpcProvider,
  token: string,
  authorizer: string,
  nonce: string
): Promise<boolean> {
  const data = TOKEN.encodeFunctionData(AUTHORIZATION_STATE, [authorizer, nonce]);
  const result = await provider.call({ to: token, data });
  const decoded = TOKEN.decodeFunctionResult(AUTHORIZATION_STATE, result);
  return decoded[0] === true;
}

function splitSignature(signature: string): { v: number; r: string; s: string } {
  return {
    r: `0x${signature.slice(2, 66)}`,
    s: `0x${signature.slice(66, 130)}`,
    v: Number.parseInt(signature.slice(130, 132), 16),
  };
}

function parseFacilitatorBody(input: unknown): FacilitatorBody {
  const root = expectRecord(input, "body");
  return {
    x402Version: expectNumber(root.x402Version, "x402Version"),
    paymentPayload: parsePaymentPayload(root.paymentPayload),
    paymentRequirements: parsePaymentRequirements(root.paymentRequirements),
  };
}

function parsePaymentPayload(input: unknown): PaymentPayload {
  const root = expectRecord(input, "paymentPayload");
  const payload = expectRecord(root.payload, "paymentPayload.payload");
  return {
    x402Version: expectNumber(root.x402Version, "paymentPayload.x402Version"),
    accepted: parsePaymentRequirements(root.accepted),
    payload: {
      signature: expectString(payload.signature, "paymentPayload.payload.signature"),
      authorization: parseAuthorization(payload.authorization),
    },
  };
}

function parsePaymentRequirements(input: unknown): PaymentRequirements {
  const root = expectRecord(input, "paymentRequirements");
  const extra = expectRecord(root.extra, "paymentRequirements.extra");
  return {
    scheme: expectString(root.scheme, "scheme"),
    network: expectString(root.network, "network"),
    asset: expectString(root.asset, "asset"),
    amount: expectString(root.amount, "amount"),
    payTo: expectString(root.payTo, "payTo"),
    maxTimeoutSeconds: expectNumber(root.maxTimeoutSeconds, "maxTimeoutSeconds"),
    extra: {
      name: expectString(extra.name, "extra.name"),
      version: expectString(extra.version, "extra.version"),
    },
  };
}

function parseAuthorization(input: unknown): Authorization {
  const root = expectRecord(input, "authorization");
  return {
    from: expectString(root.from, "authorization.from"),
    to: expectString(root.to, "authorization.to"),
    value: expectString(root.value, "authorization.value"),
    validAfter: expectString(root.validAfter, "authorization.validAfter"),
    validBefore: expectString(root.validBefore, "authorization.validBefore"),
    nonce: expectString(root.nonce, "authorization.nonce"),
  };
}

function expectRecord(value: unknown, label: string): Record<string, unknown> {
  if (!isRecord(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function expectString(value: unknown, label: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} must be a non-empty string`);
  }
  return value;
}

function expectNumber(value: unknown, label: string): number {
  if (typeof value !== "number" || !Number.isInteger(value)) {
    throw new Error(`${label} must be an integer`);
  }
  return value;
}

function sameLower(left: string, right: string): boolean {
  return left.toLowerCase() === right.toLowerCase();
}

function isHexAddress(value: string): boolean {
  return /^0x[0-9a-fA-F]{40}$/.test(value);
}

function isUintText(value: string): boolean {
  return /^(0|[1-9][0-9]*)$/.test(value);
}

function invalid(payer: string, reason: string, message: string): VerifyResponse {
  return {
    isValid: false,
    payer,
    invalidReason: reason,
    invalidMessage: message,
    extra: {},
  };
}

function zeroTxHash(): string {
  return `0x${"0".repeat(64)}`;
}

function errMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function jsonResponse(status: number, payload: unknown): RpcHttpResponse {
  return {
    status,
    headers: { "content-type": "application/json" },
    body: payload === null ? null : JSON.stringify(payload),
  };
}
