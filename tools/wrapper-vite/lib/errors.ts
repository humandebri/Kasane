// どこで: APIエラー整形 / 何を: 例外をHTTPエラーJSONへ正規化 / なぜ: 失敗時の契約を統一するため

import type { ApiErrorBody } from "./types";

export class ApiError extends Error {
  public readonly status: number;
  public readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

export function toApiError(error: unknown, fallbackCode: string): ApiError {
  if (error instanceof ApiError) {
    return error;
  }
  if (error instanceof Error) {
    if (error.message.startsWith("validation.")) {
      return new ApiError(400, error.message, error.message);
    }
    if (error.message.startsWith("config.missing:")) {
      return new ApiError(500, "config_missing", error.message);
    }
    if (error.message.startsWith("request_id.")) {
      return new ApiError(400, error.message, error.message);
    }
    return new ApiError(502, fallbackCode, error.message);
  }
  return new ApiError(500, fallbackCode, "unknown_error");
}

export function toErrorBody(error: ApiError): ApiErrorBody {
  return {
    ok: false,
    errorCode: error.code,
    message: error.message,
  };
}
