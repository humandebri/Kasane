// どこで: アーカイブGC / 何を: DBに紐づかないファイル削除 / なぜ: キャッシュの肥大化を防ぐため

import { promises as fs } from "node:fs";
import path from "node:path";
import type { IndexerDb } from "./db";

export async function runArchiveGc(db: IndexerDb, archiveDir: string, chainId: string): Promise<void> {
  const root = path.join(archiveDir, chainId);
  const files = await collectFiles(root);
  const referencedRaw = db.listArchivePaths();
  const referenced = normalizeReferenced(root, referencedRaw);
  const canDeleteOrphans = referenced.size > 0;
  for (const file of files) {
    if (file.endsWith(".tmp")) {
      await removeFile(file);
      continue;
    }
    if (!file.endsWith(".bundle.zst")) {
      continue;
    }
    if (!canDeleteOrphans) {
      continue;
    }
    const rel = path.relative(root, file);
    if (!referenced.has(rel)) {
      await removeFile(file);
    }
  }
}

async function collectFiles(root: string): Promise<string[]> {
  try {
    const stats = await fs.stat(root);
    if (!stats.isDirectory()) {
      return [];
    }
  } catch {
    return [];
  }
  const out: string[] = [];
  const queue = [root];
  while (queue.length > 0) {
    const current = queue.pop();
    if (!current) {
      break;
    }
    const entries = await fs.readdir(current, { withFileTypes: true });
    for (const entry of entries) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        queue.push(full);
        continue;
      }
      if (entry.isFile()) {
        out.push(full);
      }
    }
  }
  return out;
}

async function removeFile(filePath: string): Promise<void> {
  try {
    await fs.unlink(filePath);
  } catch {
    // GCはベストエフォート
  }
}

function normalizeReferenced(root: string, paths: Set<string>): Set<string> {
  const out = new Set<string>();
  const rootAbs = path.resolve(root);
  for (const raw of paths) {
    if (!raw) {
      continue;
    }
    const resolved = path.resolve(raw);
    if (resolved.startsWith(rootAbs + path.sep)) {
      const rel = path.relative(rootAbs, resolved);
      if (rel) {
        out.add(rel);
      }
      continue;
    }
    const rel = path.normalize(raw);
    if (!rel.startsWith("..")) {
      out.add(rel);
    }
  }
  return out;
}
