/// <reference types="node" />
/// <reference types="better-sqlite3" />

export {};

declare const process: {
  pid: number;
  exit(code?: number): never;
  on(event: string, listener: (...args: unknown[]) => void): void;
  stderr: { write(chunk: string): void };
};

declare namespace NodeJS {
  type Signals = "SIGINT" | "SIGTERM";
}

declare class Buffer extends Uint8Array {
  static from(data: Uint8Array | string, encoding?: string): Buffer;
  static alloc(size: number): Buffer;
  static allocUnsafe(size: number): Buffer;
  writeUInt32BE(value: number, offset: number): number;
  readUInt32BE(offset: number): number;
  subarray(start?: number, end?: number): Buffer;
  toString(encoding?: string): string;
}
