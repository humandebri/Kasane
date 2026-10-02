export class PayloadTooLargeError extends Error {}

export async function readBoundedJson(request: Request, maxBytes: number): Promise<unknown> {
  const length = request.headers.get("content-length");
  if (length !== null && Number(length) > maxBytes) {
    throw new PayloadTooLargeError("payload too large");
  }
  if (!request.body) {
    throw new SyntaxError("empty body");
  }
  const reader = request.body.getReader();
  const decoder = new TextDecoder();
  const parts: string[] = [];
  let bytes = 0;
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > maxBytes) {
        await reader.cancel();
        throw new PayloadTooLargeError("payload too large");
      }
      parts.push(decoder.decode(value, { stream: true }));
    }
    parts.push(decoder.decode());
    return JSON.parse(parts.join(""));
  } finally {
    reader.releaseLock();
  }
}
