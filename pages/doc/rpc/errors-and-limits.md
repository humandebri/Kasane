# Errors & Limits

## TL;DR
- HTTP側制限（body/batch/depth）とRPC側制限（blockTag/filter/range）の二層で制御。
- エラーコードは `-32602/-32000/-32001/-32005` が中心。

## Gateway HTTP制限
- `RPC_GATEWAY_MAX_HTTP_BODY_SIZE`
- `RPC_GATEWAY_MAX_BATCH_LEN`
- `RPC_GATEWAY_MAX_JSON_DEPTH`

## RPCエラー
- `-32602 invalid params`
- `-32000 state unavailable / execution failed`
- `-32001 resource not found`
- `-32005 limit exceeded`

## 根拠
- `tools/rpc-gateway/src/server.ts`
- `tools/rpc-gateway/src/handlers.ts`
- `tools/rpc-gateway/src/config.ts`
